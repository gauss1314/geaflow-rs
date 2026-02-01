use geaflow_common::error::GeaFlowResult;
use std::any::Any;

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub success: bool,
    pub error_message: Option<String>,
}

impl PipelineResult {
    pub fn success() -> Self {
        Self {
            success: true,
            error_message: None,
        }
    }

    pub fn failure(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            error_message: Some(msg.into()),
        }
    }
}

pub trait PipelineTaskContext: Send {
    fn name(&self) -> &str;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub trait PipelineTask: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn run(&self, ctx: &mut dyn PipelineTaskContext) -> GeaFlowResult<()>;
}
