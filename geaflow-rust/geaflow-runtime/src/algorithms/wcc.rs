use geaflow_api::function::{
    VertexCentricComputeAlgorithm, VertexCentricComputeFuncContext, VertexCentricComputeFunction,
};

#[derive(Clone)]
pub struct WccAlgorithm {
    pub iterations: u64,
}

impl WccAlgorithm {
    pub fn new(iterations: u64) -> Self {
        Self { iterations }
    }
}

#[derive(Clone)]
pub struct WccFunction;

impl VertexCentricComputeFunction<u64, u64, u8, u64> for WccFunction {
    fn compute(
        &mut self,
        vertex_id: &u64,
        message_iterator: &mut dyn Iterator<Item = u64>,
        context: &mut dyn VertexCentricComputeFuncContext<u64, u64, u8, u64>,
    ) {
        let current = context.vertex_value().cloned().unwrap_or(*vertex_id);

        if context.iteration() == 1 {
            context.set_new_vertex_value(*vertex_id);
            let msg = *vertex_id;
            let edges: Vec<_> = context.edges().cloned().collect();
            for e in edges {
                context.send_message(e.target_id, msg);
            }
            return;
        }

        let mut min_comp = current;
        for m in message_iterator {
            if m < min_comp {
                min_comp = m;
            }
        }

        if min_comp < current {
            context.set_new_vertex_value(min_comp);
            let edges: Vec<_> = context.edges().cloned().collect();
            for e in edges {
                context.send_message(e.target_id, min_comp);
            }
        }
    }
}

impl VertexCentricComputeAlgorithm<u64, u64, u8, u64> for WccAlgorithm {
    fn name(&self) -> &str {
        "wcc"
    }

    fn iterations(&self) -> u64 {
        self.iterations
    }

    fn create_function(&self) -> Box<dyn VertexCentricComputeFunction<u64, u64, u8, u64>> {
        Box::new(WccFunction)
    }
}
