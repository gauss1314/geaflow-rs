use clap::{Parser, Subcommand};
use dashmap::DashMap;
use geaflow_common::error::GeaFlowResult;
use geaflow_runtime::distributed::driver::DistributedDriver;
use geaflow_runtime::distributed::worker::{run_worker, WorkerConfig};
use geaflow_runtime::http::{serve_http_v2, HttpRequest, HttpResponse};
use geaflow_runtime::observability::init_tracing;
use geaflow_runtime::plan::job_spec::{
    AlgorithmSpec, CheckpointSpec, FileSource, GraphSpec, JobMode, JobSpec,
};
use geaflow_runtime::scheduler::cycle_scheduler::CycleScheduler;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Extract {
        #[arg(
            long,
            default_value = "/Users/gauss/workspace/github_project/geaflow-rs/graph500-test/graph500-22.tar.zst"
        )]
        archive: PathBuf,
        #[arg(long, default_value = "/tmp/graph500-22")]
        out_dir: PathBuf,
    },
    Inspect {
        #[arg(long, default_value = "/tmp/graph500-22")]
        dir: PathBuf,
    },
    RunWcc {
        #[arg(long, default_value = "/tmp/graph500-22")]
        dir: PathBuf,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value = "/tmp/geaflow-graph500")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 50)]
        iterations: u64,
    },
    RunPagerank {
        #[arg(long, default_value = "/tmp/graph500-22")]
        dir: PathBuf,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value = "/tmp/geaflow-graph500")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        iterations: u64,
        #[arg(long, default_value_t = 0.85)]
        alpha: f64,
    },
    RunAll {
        #[arg(long, default_value = "/tmp/graph500-22")]
        dir: PathBuf,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value = "/tmp/geaflow-graph500")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 50)]
        wcc_iterations: u64,
        #[arg(long, default_value_t = 10)]
        pr_iterations: u64,
        #[arg(long, default_value_t = 0.85)]
        alpha: f64,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1:18080")]
        listen: SocketAddr,
        #[arg(long, default_value = "/tmp/geaflow-graph500-http")]
        state_dir: PathBuf,
    },
}

fn extract_tar_zst(archive: &Path, out_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let f = File::open(archive)?;
    let decoder = zstd::stream::read::Decoder::new(f)?;
    let mut ar = tar::Archive::new(decoder);
    ar.unpack(out_dir)?;
    Ok(())
}

fn list_files_recursive(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fn walk(acc: &mut Vec<PathBuf>, p: &Path) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(p)? {
            let entry = entry?;
            let path = entry.path();
            let meta = entry.metadata()?;
            if meta.is_dir() {
                walk(acc, &path)?;
            } else if meta.is_file() {
                acc.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&mut out, dir)?;
    out.sort();
    Ok(out)
}

fn sniff_text_format(path: &Path) -> anyhow::Result<String> {
    let f = File::open(path)?;
    let mut r = BufReader::new(f);
    let mut line = String::new();
    for _ in 0..50 {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            break;
        }
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let delim = if s.contains(',') {
            "comma"
        } else if s.contains('\t') {
            "tab"
        } else {
            "space"
        };
        let parts: Vec<&str> = match delim {
            "comma" => s.split(',').collect(),
            "tab" => s.split('\t').collect(),
            _ => s.split_whitespace().collect(),
        };
        return Ok(format!("{delim}; columns={}", parts.len()));
    }
    Ok("unknown".to_string())
}

#[derive(Debug, Clone)]
struct Graph500Meta {
    vertex_file: PathBuf,
    edge_file: PathBuf,
    scale: u32,
    vertices: usize,
    edges: u64,
    directed: bool,
    wcc_truth: Option<PathBuf>,
    pr_truth: Option<PathBuf>,
}

fn read_properties(dir: &Path) -> anyhow::Result<Graph500Meta> {
    read_properties_path(&dir.join("graph500-22.properties"))
}

fn read_properties_path(prop_path: &Path) -> anyhow::Result<Graph500Meta> {
    let dir = prop_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("bad properties path"))?;
    let f = File::open(&prop_path)?;
    let r = BufReader::new(f);
    let mut kv = std::collections::HashMap::<String, String>::new();
    for line in r.lines() {
        let line = line?;
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = s.split_once('=') {
            kv.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let scale: u32 = {
        let file_name = prop_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if let Some(rest) = file_name.strip_prefix("graph500-") {
            if let Some((scale_s, _)) = rest.split_once('.') {
                scale_s.parse()?
            } else {
                0
            }
        } else {
            0
        }
    };
    let scale = if scale > 0 {
        scale
    } else {
        let mut inferred: Option<u32> = None;
        for k in kv.keys() {
            if let Some(pos) = k.find("graph.graph500-") {
                let rest = &k[(pos + "graph.graph500-".len())..];
                if let Some((scale_s, _)) = rest.split_once('.') {
                    if let Ok(s) = scale_s.parse::<u32>() {
                        inferred = Some(s);
                        break;
                    }
                }
            }
        }
        inferred.ok_or_else(|| anyhow::anyhow!("failed to infer graph500 scale"))?
    };

    let prefix = format!("graph.graph500-{scale}");
    let vertex_file = kv
        .get(&format!("{prefix}.vertex-file"))
        .ok_or_else(|| anyhow::anyhow!("missing vertex-file in properties"))?;
    let edge_file = kv
        .get(&format!("{prefix}.edge-file"))
        .ok_or_else(|| anyhow::anyhow!("missing edge-file in properties"))?;
    let vertices: usize = kv
        .get(&format!("{prefix}.meta.vertices"))
        .ok_or_else(|| anyhow::anyhow!("missing meta.vertices in properties"))?
        .parse()?;
    let edges: u64 = kv
        .get(&format!("{prefix}.meta.edges"))
        .ok_or_else(|| anyhow::anyhow!("missing meta.edges in properties"))?
        .parse()?;
    let directed: bool = kv
        .get(&format!("{prefix}.directed"))
        .map(|s| s == "true")
        .unwrap_or(false);

    let wcc_truth = {
        let p = dir.join(format!("graph500-{scale}-WCC"));
        if p.exists() {
            Some(p)
        } else {
            None
        }
    };
    let pr_truth = {
        let p = dir.join(format!("graph500-{scale}-PR"));
        if p.exists() {
            Some(p)
        } else {
            None
        }
    };

    Ok(Graph500Meta {
        vertex_file: dir.join(vertex_file),
        edge_file: dir.join(edge_file),
        scale,
        vertices,
        edges,
        directed,
        wcc_truth,
        pr_truth,
    })
}

fn free_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

fn gf<T>(r: GeaFlowResult<T>) -> anyhow::Result<T> {
    r.map_err(|e| anyhow::anyhow!("{e}"))
}

async fn start_workers(
    workers: usize,
    base_dir: &Path,
) -> anyhow::Result<(Vec<SocketAddr>, Vec<tokio::task::JoinHandle<()>>)> {
    let mut addrs = Vec::with_capacity(workers);
    let mut handles = Vec::with_capacity(workers);
    std::fs::create_dir_all(base_dir)?;

    for i in 0..workers {
        let addr = free_local_addr();
        let state_dir = base_dir.join(format!("worker_{i}"));
        if state_dir.exists() {
            let _ = std::fs::remove_dir_all(&state_dir);
        }
        std::fs::create_dir_all(&state_dir)?;
        let handle = tokio::spawn(async move {
            let _ = run_worker(WorkerConfig {
                listen_addr: addr,
                state_dir,
                master_addr: None,
            })
            .await;
        });
        addrs.push(addr);
        handles.push(handle);
    }
    Ok((addrs, handles))
}

fn load_truth_wcc(path: &Path) -> anyhow::Result<std::collections::HashMap<u64, u64>> {
    let f = File::open(path)?;
    let r = BufReader::new(f);
    let mut expected = std::collections::HashMap::new();
    for line in r.lines() {
        let line = line?;
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let mut it = s.split_whitespace();
        let id: u64 = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("bad wcc line"))?
            .parse()?;
        let comp: u64 = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("bad wcc line"))?
            .parse()?;
        expected.insert(id, comp);
    }
    Ok(expected)
}

fn verify_wcc_csv_parts(
    parts: &[PathBuf],
    mut expected: std::collections::HashMap<u64, u64>,
) -> anyhow::Result<(u64, u64, u64, u64)> {
    let mut total = 0u64;
    let mut mismatches = 0u64;
    let mut unexpected = 0u64;
    for p in parts {
        let f = File::open(p)?;
        let r = BufReader::new(f);
        for line in r.lines() {
            let line = line?;
            let s = line.trim();
            if s.is_empty() {
                continue;
            }
            let (id_s, v_s) = s
                .split_once(',')
                .ok_or_else(|| anyhow::anyhow!("bad output line (expect csv): {s}"))?;
            let id: u64 = id_s.parse()?;
            let v: u64 = v_s.parse()?;
            total += 1;
            match expected.remove(&id) {
                None => {
                    unexpected += 1;
                    if unexpected <= 10 {
                        eprintln!("WCC unexpected vertex: id={id} got={v}");
                    }
                }
                Some(ev) => {
                    if ev != v {
                        mismatches += 1;
                        if mismatches <= 10 {
                            eprintln!("WCC mismatch: id={id} expected={ev} got={v}");
                        }
                    }
                }
            }
        }
    }
    let missing = expected.len() as u64;
    Ok((total, mismatches, unexpected, missing))
}

fn load_truth_pr(path: &Path) -> anyhow::Result<std::collections::HashMap<u64, f64>> {
    let f = File::open(path)?;
    let r = BufReader::new(f);
    let mut expected = std::collections::HashMap::new();
    for line in r.lines() {
        let line = line?;
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let mut it = s.split_whitespace();
        let id: u64 = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("bad pr line"))?
            .parse()?;
        let v: f64 = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("bad pr line"))?
            .parse()?;
        expected.insert(id, v);
    }
    Ok(expected)
}

fn verify_pr_csv_parts(
    parts: &[PathBuf],
    mut expected: std::collections::HashMap<u64, f64>,
) -> anyhow::Result<(u64, u64, u64, f64, f64)> {
    let mut total = 0u64;
    let mut max_abs = 0f64;
    let mut l1 = 0f64;
    let mut unexpected = 0u64;
    for p in parts {
        let f = File::open(p)?;
        let r = BufReader::new(f);
        for line in r.lines() {
            let line = line?;
            let s = line.trim();
            if s.is_empty() {
                continue;
            }
            let (id_s, v_s) = s
                .split_once(',')
                .ok_or_else(|| anyhow::anyhow!("bad output line (expect csv): {s}"))?;
            let id: u64 = id_s.parse()?;
            let v: f64 = v_s.parse()?;
            match expected.remove(&id) {
                None => {
                    unexpected += 1;
                    if unexpected <= 10 {
                        eprintln!("PR unexpected vertex: id={id} got={v}");
                    }
                }
                Some(e) => {
                    let diff = (v - e).abs();
                    if diff > max_abs {
                        max_abs = diff;
                    }
                    l1 += diff;
                }
            }
            total += 1;
        }
    }
    let missing = expected.len() as u64;
    Ok((total, unexpected, missing, max_abs, l1))
}

#[derive(Debug, Clone, serde::Serialize)]
struct WccReport {
    vertices_seen: u64,
    mismatches: u64,
    unexpected: u64,
    missing: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PageRankReport {
    vertices_seen: u64,
    unexpected: u64,
    missing: u64,
    max_abs_diff: f64,
    l1_diff: f64,
}

async fn run_wcc_meta(
    meta: &Graph500Meta,
    workers: usize,
    out_dir: &Path,
    iterations: u64,
) -> anyhow::Result<WccReport> {
    let vertex_count: u64 = 1u64 << meta.scale;
    println!(
        "Graph500 meta: scale={} vertex_count={} meta.vertices={} edges={} directed={}",
        meta.scale, vertex_count, meta.vertices, meta.edges, meta.directed
    );
    let undirected = !meta.directed;

    let db_dir = out_dir.join("db_wcc");
    let out_wcc_dir = out_dir.join("out_wcc");
    let _ = std::fs::remove_dir_all(&db_dir);
    let _ = std::fs::remove_dir_all(&out_wcc_dir);
    let (worker_addrs, handles) = start_workers(workers, &db_dir).await?;

    let mut driver = gf(DistributedDriver::connect(&worker_addrs).await)?;
    let t0 = std::time::Instant::now();
    gf(driver
        .load_graph500_streaming(
            &meta.vertex_file,
            &meta.edge_file,
            |id| bincode::serialize(&id).unwrap(),
            50_000,
            50_000,
            undirected,
        )
        .await)?;
    println!("WCC load done in {:.2}s", t0.elapsed().as_secs_f64());

    let job = JobSpec {
        job_id: "graph500_wcc".to_string(),
        name: "wcc".to_string(),
        mode: JobMode::Distributed,
        graph: GraphSpec {
            vertices: FileSource::Csv {
                path: meta.vertex_file.to_string_lossy().to_string(),
            },
            edges: FileSource::Csv {
                path: meta.edge_file.to_string_lossy().to_string(),
            },
        },
        algorithm: AlgorithmSpec::Wcc { iterations },
        checkpoint: CheckpointSpec {
            enabled: false,
            interval_iters: 0,
            base_dir: "/tmp/geaflow-checkpoints".to_string(),
        },
    };

    gf(driver
        .set_algorithm("wcc".to_string(), iterations, Vec::new())
        .await)?;
    let t1 = std::time::Instant::now();
    gf(CycleScheduler::run(&mut driver, &job).await)?;
    println!("WCC compute done in {:.2}s", t1.elapsed().as_secs_f64());

    let out_parts = gf(driver.dump_vertices_csv(&out_wcc_dir, "wcc").await)?;

    let t2 = std::time::Instant::now();
    let truth_path = meta
        .wcc_truth
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing WCC truth file"))?;
    let expected = load_truth_wcc(truth_path)?;
    let (total, mismatches, unexpected, missing) = verify_wcc_csv_parts(&out_parts, expected)?;
    println!(
        "WCC verify: vertices_seen={total} mismatches={mismatches} unexpected={unexpected} missing={missing} (truth={})",
        truth_path.display(),
    );
    println!("WCC verify done in {:.2}s", t2.elapsed().as_secs_f64());

    let _ = driver.shutdown().await;
    for h in handles {
        h.abort();
    }

    Ok(WccReport {
        vertices_seen: total,
        mismatches,
        unexpected,
        missing,
    })
}

async fn run_pagerank_meta(
    meta: &Graph500Meta,
    workers: usize,
    out_dir: &Path,
    iterations: u64,
    alpha: f64,
) -> anyhow::Result<PageRankReport> {
    let vertex_count: u64 = 1u64 << meta.scale;
    println!(
        "Graph500 meta: scale={} vertex_count={} meta.vertices={} edges={} directed={}",
        meta.scale, vertex_count, meta.vertices, meta.edges, meta.directed
    );
    let undirected = !meta.directed;
    let db_dir = out_dir.join("db_pr");
    let out_pr_dir = out_dir.join("out_pr");
    let _ = std::fs::remove_dir_all(&db_dir);
    let _ = std::fs::remove_dir_all(&out_pr_dir);
    let (worker_addrs, handles) = start_workers(workers, &db_dir).await?;

    let init = 1.0f64 / (meta.vertices as f64);
    let mut driver = gf(DistributedDriver::connect(&worker_addrs).await)?;
    let t0 = std::time::Instant::now();
    gf(driver
        .load_graph500_streaming(
            &meta.vertex_file,
            &meta.edge_file,
            |_| bincode::serialize(&init).unwrap(),
            50_000,
            50_000,
            undirected,
        )
        .await)?;
    println!("PR load done in {:.2}s", t0.elapsed().as_secs_f64());

    let job = JobSpec {
        job_id: "graph500_pr".to_string(),
        name: "pagerank".to_string(),
        mode: JobMode::Distributed,
        graph: GraphSpec {
            vertices: FileSource::Csv {
                path: meta.vertex_file.to_string_lossy().to_string(),
            },
            edges: FileSource::Csv {
                path: meta.edge_file.to_string_lossy().to_string(),
            },
        },
        algorithm: AlgorithmSpec::PageRank { iterations, alpha },
        checkpoint: CheckpointSpec {
            enabled: false,
            interval_iters: 0,
            base_dir: "/tmp/geaflow-checkpoints".to_string(),
        },
    };

    let params = geaflow_runtime::distributed::algorithm::PageRankParams { alpha };
    gf(driver
        .set_algorithm(
            "pagerank".to_string(),
            iterations,
            bincode::serialize(&params).unwrap(),
        )
        .await)?;
    let t1 = std::time::Instant::now();
    gf(CycleScheduler::run(&mut driver, &job).await)?;
    println!("PR compute done in {:.2}s", t1.elapsed().as_secs_f64());

    let out_parts = gf(driver.dump_vertices_csv(&out_pr_dir, "pr").await)?;

    let t2 = std::time::Instant::now();
    let truth_path = meta
        .pr_truth
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing PR truth file"))?;
    let expected = load_truth_pr(truth_path)?;
    let (total, unexpected, missing, max_abs, l1) = verify_pr_csv_parts(&out_parts, expected)?;
    println!(
        "PR verify: vertices_seen={total} unexpected={unexpected} missing={missing} max_abs_diff={max_abs:.3e} l1_diff={l1:.3e} (truth={})",
        truth_path.display(),
    );
    println!("PR verify done in {:.2}s", t2.elapsed().as_secs_f64());

    let _ = driver.shutdown().await;
    for h in handles {
        h.abort();
    }

    Ok(PageRankReport {
        vertices_seen: total,
        unexpected,
        missing,
        max_abs_diff: max_abs,
        l1_diff: l1,
    })
}

async fn run_wcc(
    dir: &Path,
    workers: usize,
    out_dir: &Path,
    iterations: u64,
) -> anyhow::Result<()> {
    let meta = read_properties(dir)?;
    let report = run_wcc_meta(&meta, workers, out_dir, iterations).await?;
    if report.mismatches > 0 || report.unexpected > 0 || report.missing > 0 {
        return Err(anyhow::anyhow!("WCC verification failed"));
    }
    Ok(())
}

async fn run_pagerank(
    dir: &Path,
    workers: usize,
    out_dir: &Path,
    iterations: u64,
    alpha: f64,
) -> anyhow::Result<()> {
    let meta = read_properties(dir)?;
    let report = run_pagerank_meta(&meta, workers, out_dir, iterations, alpha).await?;
    if report.unexpected > 0
        || report.missing > 0
        || !report.max_abs_diff.is_finite()
        || report.max_abs_diff > 1e-6
    {
        return Err(anyhow::anyhow!(
            "PageRank verification failed: max_abs_diff too large"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct DatasetEntry {
    meta: Graph500Meta,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize)]
struct JobRecord {
    job_id: String,
    dataset_id: String,
    status: JobStatus,
    submitted_ms: u64,
    started_ms: Option<u64>,
    finished_ms: Option<u64>,
    message: Option<String>,
    out_dir: String,
    wcc: Option<WccReport>,
    pagerank: Option<PageRankReport>,
}

#[derive(Debug)]
struct AppState {
    state_dir: PathBuf,
    datasets: DashMap<String, DatasetEntry>,
    jobs: DashMap<String, JobRecord>,
}

static JOB_NONCE: AtomicU64 = AtomicU64::new(1);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn json_response<T: serde::Serialize>(status: u16, v: &T) -> HttpResponse {
    match serde_json::to_vec(v) {
        Ok(body) => HttpResponse {
            status,
            content_type: "application/json",
            body,
        },
        Err(e) => HttpResponse {
            status: 500,
            content_type: "text/plain",
            body: format!("{e}").into_bytes(),
        },
    }
}

fn text_response(status: u16, s: impl Into<String>) -> HttpResponse {
    HttpResponse {
        status,
        content_type: "text/plain",
        body: s.into().into_bytes(),
    }
}

#[derive(Debug, serde::Deserialize)]
struct RegisterDatasetRequest {
    dataset_id: String,
    properties_path: String,
    vertex_path: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DatasetInfo {
    dataset_id: String,
    properties_path: String,
    vertex_path: String,
    edge_path: String,
    scale: u32,
    vertices: usize,
    edges: u64,
    directed: bool,
    wcc_truth_path: Option<String>,
    pr_truth_path: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SubmitJobRequest {
    dataset_id: String,
    workers: usize,
    wcc_iterations: u64,
    pr_iterations: u64,
    alpha: f64,
    run: Vec<String>,
    out_dir: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct SubmitJobResponse {
    job_id: String,
}

async fn serve_graph500_http(listen: SocketAddr, state_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&state_dir)?;
    let state = Arc::new(AppState {
        state_dir,
        datasets: DashMap::new(),
        jobs: DashMap::new(),
    });

    let handler: geaflow_runtime::http::HttpHandlerV2 = Arc::new(move |req: HttpRequest| {
        let state = state.clone();
        Box::pin(async move { handle_http(state, req).await })
    });

    gf(serve_http_v2(listen, handler).await)?;
    Ok(())
}

async fn handle_http(state: Arc<AppState>, req: HttpRequest) -> HttpResponse {
    let path = req.path.split('?').next().unwrap_or(&req.path);
    match (req.method.as_str(), path) {
        ("GET", "/healthz") => text_response(200, "ok"),
        ("POST", "/api/v1/datasets/register") => handle_register_dataset(state, &req).await,
        ("POST", "/api/v1/graph500/jobs") => handle_submit_job(state, &req).await,
        ("GET", "/api/v1/graph500/jobs") => handle_list_jobs(state).await,
        _ => {
            if req.method == "GET" && path.starts_with("/api/v1/graph500/jobs/") {
                let job_id = path.trim_start_matches("/api/v1/graph500/jobs/");
                return handle_get_job(state, job_id).await;
            }
            text_response(404, "not found")
        }
    }
}

async fn handle_register_dataset(state: Arc<AppState>, req: &HttpRequest) -> HttpResponse {
    let payload: RegisterDatasetRequest = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return text_response(400, format!("bad json: {e}")),
    };
    let prop_path = PathBuf::from(payload.properties_path);
    let mut meta = match read_properties_path(&prop_path) {
        Ok(m) => m,
        Err(e) => return text_response(400, format!("{e}")),
    };
    if let Some(vp) = payload.vertex_path.as_ref() {
        meta.vertex_file = PathBuf::from(vp);
    }
    if !meta.vertex_file.exists() {
        return text_response(
            400,
            format!("vertex file not found: {}", meta.vertex_file.display()),
        );
    }
    if !meta.edge_file.exists() {
        return text_response(
            400,
            format!("edge file not found: {}", meta.edge_file.display()),
        );
    }

    let dataset = DatasetEntry { meta: meta.clone() };
    state.datasets.insert(payload.dataset_id.clone(), dataset);

    let info = DatasetInfo {
        dataset_id: payload.dataset_id,
        properties_path: prop_path.to_string_lossy().to_string(),
        vertex_path: meta.vertex_file.to_string_lossy().to_string(),
        edge_path: meta.edge_file.to_string_lossy().to_string(),
        scale: meta.scale,
        vertices: meta.vertices,
        edges: meta.edges,
        directed: meta.directed,
        wcc_truth_path: meta.wcc_truth.map(|p| p.to_string_lossy().to_string()),
        pr_truth_path: meta.pr_truth.map(|p| p.to_string_lossy().to_string()),
    };
    json_response(200, &info)
}

async fn handle_submit_job(state: Arc<AppState>, req: &HttpRequest) -> HttpResponse {
    let payload: SubmitJobRequest = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return text_response(400, format!("bad json: {e}")),
    };
    let dataset = match state.datasets.get(&payload.dataset_id) {
        Some(d) => d.clone(),
        None => return text_response(404, "dataset not found"),
    };

    let job_id = format!("graph500-{}", JOB_NONCE.fetch_add(1, Ordering::SeqCst));
    let job_out_dir = PathBuf::from(
        payload
            .out_dir
            .unwrap_or_else(|| state.state_dir.to_string_lossy().to_string()),
    )
    .join("jobs")
    .join(&job_id);
    let job_out_dir_s = job_out_dir.to_string_lossy().to_string();
    let run = payload.run.clone();

    let record = JobRecord {
        job_id: job_id.clone(),
        dataset_id: payload.dataset_id.clone(),
        status: JobStatus::Pending,
        submitted_ms: now_ms(),
        started_ms: None,
        finished_ms: None,
        message: None,
        out_dir: job_out_dir_s.clone(),
        wcc: None,
        pagerank: None,
    };
    state.jobs.insert(job_id.clone(), record);

    let state_cloned = state.clone();
    let job_id_resp = job_id.clone();
    tokio::spawn(async move {
        {
            if let Some(mut j) = state_cloned.jobs.get_mut(&job_id) {
                j.status = JobStatus::Running;
                j.started_ms = Some(now_ms());
            }
        }

        let result = async {
            std::fs::create_dir_all(&job_out_dir)?;
            let mut wcc_report = None;
            let mut pr_report = None;

            for item in &run {
                if item == "wcc" {
                    wcc_report = Some(
                        run_wcc_meta(
                            &dataset.meta,
                            payload.workers,
                            &job_out_dir,
                            payload.wcc_iterations,
                        )
                        .await?,
                    );
                } else if item == "pr" || item == "pagerank" {
                    pr_report = Some(
                        run_pagerank_meta(
                            &dataset.meta,
                            payload.workers,
                            &job_out_dir,
                            payload.pr_iterations,
                            payload.alpha,
                        )
                        .await?,
                    );
                }
            }

            if let Some(w) = wcc_report.as_ref() {
                if w.mismatches > 0 || w.unexpected > 0 || w.missing > 0 {
                    return Err(anyhow::anyhow!("WCC verification failed"));
                }
            }
            if let Some(p) = pr_report.as_ref() {
                if p.unexpected > 0
                    || p.missing > 0
                    || !p.max_abs_diff.is_finite()
                    || p.max_abs_diff > 1e-6
                {
                    return Err(anyhow::anyhow!("PageRank verification failed"));
                }
            }

            Ok::<(Option<WccReport>, Option<PageRankReport>), anyhow::Error>((
                wcc_report, pr_report,
            ))
        }
        .await;

        match result {
            Ok((wcc_report, pr_report)) => {
                if let Some(mut j) = state_cloned.jobs.get_mut(&job_id) {
                    j.status = JobStatus::Succeeded;
                    j.finished_ms = Some(now_ms());
                    j.wcc = wcc_report;
                    j.pagerank = pr_report;
                }
            }
            Err(e) => {
                if let Some(mut j) = state_cloned.jobs.get_mut(&job_id) {
                    j.status = JobStatus::Failed;
                    j.finished_ms = Some(now_ms());
                    j.message = Some(format!("{e}"));
                }
            }
        }
    });

    json_response(
        200,
        &SubmitJobResponse {
            job_id: job_id_resp,
        },
    )
}

async fn handle_list_jobs(state: Arc<AppState>) -> HttpResponse {
    let mut out: Vec<(String, JobStatus)> = state
        .jobs
        .iter()
        .map(|e| (e.key().clone(), e.value().status.clone()))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    json_response(200, &out)
}

async fn handle_get_job(state: Arc<AppState>, job_id: &str) -> HttpResponse {
    match state.jobs.get(job_id) {
        None => text_response(404, "job not found"),
        Some(v) => json_response(200, &*v),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let args = Args::parse();
    match args.cmd {
        Command::Extract { archive, out_dir } => {
            println!("Extracting {:?} -> {:?}", archive, out_dir);
            extract_tar_zst(&archive, &out_dir)?;
            println!("Done.");
        }
        Command::Inspect { dir } => {
            let files = list_files_recursive(&dir)?;
            println!("Files under {:?}:", dir);
            for p in &files {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                println!("{} ({size} bytes)", p.display());
            }

            let mut candidates: Vec<(u64, PathBuf)> = files
                .iter()
                .filter(|p| {
                    p.extension()
                        .and_then(|s| s.to_str())
                        .map(|s| matches!(s, "txt" | "csv" | "tsv" | "edges" | "el"))
                        .unwrap_or(false)
                })
                .filter_map(|p| std::fs::metadata(p).ok().map(|m| (m.len(), p.clone())))
                .collect();
            candidates.sort_by(|a, b| b.0.cmp(&a.0));

            if let Some((_, p)) = candidates.first() {
                let fmt = sniff_text_format(p)?;
                println!("Largest text-like candidate: {} ({fmt})", p.display());
            }

            let mut first_bytes = [0u8; 64];
            if let Some(p) = files.first() {
                let mut f = File::open(p)?;
                let n = f.read(&mut first_bytes)?;
                println!(
                    "First file head bytes: {}",
                    String::from_utf8_lossy(&first_bytes[..n])
                );
            }
        }
        Command::RunWcc {
            dir,
            workers,
            out_dir,
            iterations,
        } => {
            std::fs::create_dir_all(&out_dir)?;
            run_wcc(&dir, workers, &out_dir, iterations).await?;
        }
        Command::RunPagerank {
            dir,
            workers,
            out_dir,
            iterations,
            alpha,
        } => {
            std::fs::create_dir_all(&out_dir)?;
            run_pagerank(&dir, workers, &out_dir, iterations, alpha).await?;
        }
        Command::RunAll {
            dir,
            workers,
            out_dir,
            wcc_iterations,
            pr_iterations,
            alpha,
        } => {
            std::fs::create_dir_all(&out_dir)?;
            run_wcc(&dir, workers, &out_dir, wcc_iterations).await?;
            run_pagerank(&dir, workers, &out_dir, pr_iterations, alpha).await?;
        }
        Command::Serve { listen, state_dir } => {
            serve_graph500_http(listen, state_dir).await?;
        }
    }
    Ok(())
}
