use clap::{Parser, ValueEnum};
use geaflow_api::graph::PGraphWindow;
use geaflow_runtime::algorithms::pagerank::PageRankAlgorithm;
use geaflow_runtime::algorithms::wcc::WccAlgorithm;
use geaflow_runtime::distributed::protocol::{
    framed, recv_msg, send_msg, ClientToDriver, DriverToClient,
};
use geaflow_runtime::graph::partitioned_graph::PartitionedGraph;
use geaflow_runtime::io::file::{
    read_edges_u64_u8, read_vertices_u64_f64, read_vertices_u64_u64_id_default,
};
use geaflow_runtime::observability::init_tracing;
use geaflow_runtime::plan::execution_plan::ExecutionPlan;
use geaflow_runtime::plan::job_spec::{
    AlgorithmSpec, CheckpointSpec, FileSource, GraphSpec, JobMode, JobSpec,
};
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};

#[derive(Debug, Copy, Clone, ValueEnum)]
enum Mode {
    Local,
    Distributed,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum Algorithm {
    Wcc,
    Pagerank,
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, value_enum, default_value_t = Mode::Distributed)]
    mode: Mode,

    #[arg(long, value_enum)]
    algorithm: Algorithm,

    #[arg(long)]
    vertices: String,

    #[arg(long)]
    edges: String,

    #[arg(long, default_value_t = 10)]
    iterations: u64,

    #[arg(long, default_value_t = 0.85)]
    alpha: f64,

    #[arg(long, default_value_t = 4)]
    parallelism: usize,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    driver: Option<SocketAddr>,

    #[arg(long, value_delimiter = ',')]
    workers: Vec<SocketAddr>,

    #[arg(long, default_value_t = false)]
    checkpoint_enabled: bool,

    #[arg(long, default_value_t = 0)]
    checkpoint_interval_iters: u64,

    #[arg(long, default_value = "/tmp/geaflow-checkpoints")]
    checkpoint_dir: String,
}

fn dec<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    bincode::deserialize(bytes).unwrap()
}

fn new_job_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("job_{nanos}")
}

fn build_job_spec(args: &Args) -> JobSpec {
    let algorithm = match args.algorithm {
        Algorithm::Wcc => AlgorithmSpec::Wcc {
            iterations: args.iterations,
        },
        Algorithm::Pagerank => AlgorithmSpec::PageRank {
            iterations: args.iterations,
            alpha: args.alpha,
        },
    };

    let mode = match args.mode {
        Mode::Local => JobMode::Local,
        Mode::Distributed => JobMode::Distributed,
    };

    JobSpec {
        job_id: new_job_id(),
        name: format!("{:?}", args.algorithm).to_lowercase(),
        mode,
        graph: GraphSpec {
            vertices: FileSource::Csv {
                path: args.vertices.clone(),
            },
            edges: FileSource::Csv {
                path: args.edges.clone(),
            },
        },
        algorithm,
        checkpoint: CheckpointSpec {
            enabled: args.checkpoint_enabled,
            interval_iters: args.checkpoint_interval_iters,
            base_dir: args.checkpoint_dir.clone(),
        },
    }
}

async fn run_via_driver(
    addr: SocketAddr,
    job: &JobSpec,
    algorithm: Algorithm,
) -> Result<(), Box<dyn std::error::Error>> {
    let stream = TcpStream::connect(addr).await?;
    let mut framed = framed(stream);

    send_msg(
        &mut framed,
        &ClientToDriver::SubmitJob {
            job_spec: bincode::serialize(job)?,
        },
    )
    .await?;

    let resp: DriverToClient = recv_msg(&mut framed).await?;
    let job_id = match resp {
        DriverToClient::JobAccepted { job_id } => job_id,
        DriverToClient::Error { message } => return Err(message.into()),
        other => return Err(format!("unexpected response: {other:?}").into()),
    };

    loop {
        send_msg(
            &mut framed,
            &ClientToDriver::GetJobStatus {
                job_id: job_id.clone(),
            },
        )
        .await?;
        let resp: DriverToClient = recv_msg(&mut framed).await?;
        match resp {
            DriverToClient::JobStatus { state, .. } => {
                if state == "finished" {
                    break;
                }
                if state == "failed" || state == "not_found" {
                    return Err(format!("job status: {state}").into());
                }
            }
            DriverToClient::Error { message } => return Err(message.into()),
            other => return Err(format!("unexpected response: {other:?}").into()),
        }
        sleep(Duration::from_millis(50)).await;
    }

    send_msg(
        &mut framed,
        &ClientToDriver::FetchVertices {
            job_id: job_id.clone(),
        },
    )
    .await?;
    let resp: DriverToClient = recv_msg(&mut framed).await?;
    let vertices = match resp {
        DriverToClient::Vertices { vertices, .. } => vertices,
        DriverToClient::Error { message } => return Err(message.into()),
        other => return Err(format!("unexpected response: {other:?}").into()),
    };

    let mut vertices = vertices;
    vertices.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, value) in vertices {
        match algorithm {
            Algorithm::Wcc => {
                let id: u64 = dec(&id);
                let v: u64 = dec(&value);
                println!("{id},{v}");
            }
            Algorithm::Pagerank => {
                let id: u64 = dec(&id);
                let v: f64 = dec(&value);
                println!("{id},{v}");
            }
        }
    }

    send_msg(&mut framed, &ClientToDriver::Shutdown).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_tracing();
    let job = build_job_spec(&args);

    let plan = ExecutionPlan::from_job_spec(
        &job,
        if args.workers.is_empty() {
            0
        } else {
            args.workers.len()
        },
        match args.mode {
            Mode::Local => args.parallelism.max(1),
            Mode::Distributed => args.workers.len().max(1),
        },
    );
    if args.dry_run {
        println!("{}", serde_json::to_string_pretty(&job)?);
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    match args.mode {
        Mode::Local => match args.algorithm {
            Algorithm::Wcc => {
                let vertices = read_vertices_u64_u64_id_default(&args.vertices)?;
                let edges = read_edges_u64_u8(&args.edges, 0)?;
                let graph = PartitionedGraph::new(vertices, edges, args.parallelism);
                let algo = WccAlgorithm::new(args.iterations);
                let result_graph = graph.compute_algorithm(&algo, args.parallelism);
                let mut vertices = result_graph.vertices();
                vertices.sort_by_key(|v| v.id);
                for v in vertices {
                    println!("{},{}", v.id, v.value);
                }
            }
            Algorithm::Pagerank => {
                let vertices = read_vertices_u64_f64(&args.vertices, 1.0)?;
                let edges = read_edges_u64_u8(&args.edges, 0)?;
                let graph = PartitionedGraph::new(vertices, edges, args.parallelism);
                let algo = PageRankAlgorithm::new(args.iterations, args.alpha);
                let result_graph = graph.compute_algorithm(&algo, args.parallelism);
                let mut vertices = result_graph.vertices();
                vertices.sort_by_key(|v| v.id);
                for v in vertices {
                    println!("{},{}", v.id, v.value);
                }
            }
        },
        Mode::Distributed => {
            if let Some(driver_addr) = args.driver {
                run_via_driver(driver_addr, &job, args.algorithm).await?;
                return Ok(());
            }
            if args.workers.is_empty() {
                eprintln!("--workers or --driver is required in distributed mode");
                std::process::exit(2);
            }

            let vertices = geaflow_runtime::distributed::driver::DistributedDriver::run_job(
                &args.workers,
                &job,
            )
            .await?;

            let mut vertices = vertices;

            vertices.sort_by(|a, b| a.0.cmp(&b.0));
            for (id, value) in vertices {
                match args.algorithm {
                    Algorithm::Wcc => {
                        let id: u64 = dec(&id);
                        let v: u64 = dec(&value);
                        println!("{id},{v}");
                    }
                    Algorithm::Pagerank => {
                        let id: u64 = dec(&id);
                        let v: f64 = dec(&value);
                        println!("{id},{v}");
                    }
                }
            }
        }
    }

    Ok(())
}
