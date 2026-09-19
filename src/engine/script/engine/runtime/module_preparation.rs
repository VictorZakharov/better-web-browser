//! Compile/discover a module graph without linking or evaluating it.
use super::*;

impl Context {
    pub(in crate::engine::script) fn prepare_module(
        &mut self,
        url: &str,
        source: &str,
        sources: &HashMap<String, String>,
    ) -> JsResult<Vec<String>> {
        let context = self.context.clone();
        match self.agent.borrow_mut().run(|isolate| {
            super::super::modules::evaluate(isolate, &context, url, source, sources, true)
        })? {
            EngineModuleEvaluation::Missing(urls) => Ok(urls),
            EngineModuleEvaluation::Ready => Ok(Vec::new()),
            _ => unreachable!("preparation cannot evaluate JavaScript"),
        }
    }
}
