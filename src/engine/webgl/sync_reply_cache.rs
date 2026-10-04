//! Task-stable fence polls can reuse a successful native reply on the caller.
//! This cache never samples completion or lets script advance a GPU task.
use super::Command;
use serde_json::Value;
use std::collections::HashMap;

const MAX_REPLIES: usize = 256;

#[derive(Default)]
pub(super) struct Cache {
    replies: HashMap<u32, HashMap<String, Value>>,
}

impl Cache {
    pub(super) fn get(&self, context: u32, command: &str) -> Option<Value> {
        self.replies.get(&context)?.get(command).cloned()
    }

    pub(super) fn record(&mut self, context: u32, source: &str, value: &Value) {
        // Retain only successful task-frozen status and zero-flag, zero-time
        // waits. FLUSH_COMMANDS_BIT has a real submission side effect and must
        // always reach ANGLE; invalid/deleted/foreign names never populate us.
        let cacheable = source.len() <= 256
            && serde_json::from_str::<Command>(source).is_ok_and(|command| {
                match command.op.as_str() {
                    "getSyncParameter" => {
                        command.i.len() == 2
                            && command.i[1] == 0x9114
                            && matches!(value.as_u64(), Some(0x9118 | 0x9119))
                    }
                    "clientWaitSync" => {
                        command.i.len() == 3
                            && command.i[1..] == [0, 0]
                            && matches!(value.as_u64(), Some(0x911a | 0x911b))
                    }
                    _ => false,
                }
            });
        if !cacheable {
            self.remove(context);
            return;
        }
        if self.replies.values().map(HashMap::len).sum::<usize>() == MAX_REPLIES {
            self.replies.clear();
        }
        self.replies
            .entry(context)
            .or_default()
            .insert(source.into(), value.clone());
    }

    pub(super) fn remove(&mut self, context: u32) {
        self.replies.remove(&context);
    }

    pub(super) fn clear(&mut self) {
        self.replies.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn poll(id: u32) -> String {
        json!({"op":"getSyncParameter","i":[id,0x9114]}).to_string()
    }

    #[test]
    fn only_successful_side_effect_free_native_polls_are_retained() {
        let mut cache = Cache::default();
        for (source, value, admitted) in [
            (poll(1), json!(0x9118), true),
            (poll(1), json!(0x9119), true),
            (poll(1), Value::Null, false),
            (poll(1), json!({"lost":true}), false),
            (
                r#"{"op":"getSyncParameter","i":[1,0]}"#.into(),
                json!(0x9118),
                false,
            ),
            (
                r#"{"op":"clientWaitSync","i":[1,0,0]}"#.into(),
                json!(0x911b),
                true,
            ),
            (
                r#"{"op":"clientWaitSync","i":[1,1,0]}"#.into(),
                json!(0x911b),
                false,
            ),
            (
                r#"{"op":"clientWaitSync","i":[1,0,1]}"#.into(),
                json!(0x911d),
                false,
            ),
            (r#"{"op":"finish"}"#.into(), Value::Null, false),
            ("invalid JSON".into(), json!(0x9118), false),
        ] {
            cache.record(7, &source, &value);
            assert_eq!(cache.get(7, &source).is_some(), admitted, "{source}");
            assert!(cache.get(8, &source).is_none());
            cache.clear();
        }
    }

    #[test]
    fn cache_is_bounded_and_task_or_context_retirement_discards_results() {
        let mut cache = Cache::default();
        for id in 0..=MAX_REPLIES as u32 {
            cache.record(7, &poll(id), &json!(0x9118));
        }
        assert_eq!(cache.replies.len(), 1);
        cache.record(8, &poll(1), &json!(0x9119));
        cache.remove(7);
        assert_eq!(cache.get(8, &poll(1)), Some(json!(0x9119)));
        cache.clear();
        assert!(cache.replies.is_empty());
    }

    #[test]
    fn session_polls_follow_native_task_publication_and_deleted_name_validation() {
        let mut contexts = super::super::session::Contexts::default();
        let id = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
        let fence = contexts
            .execute(id, r#"{"op":"fenceSync","i":[37143,0]}"#, None)
            .as_u64()
            .unwrap() as u32;
        let source = poll(fence);
        contexts.execute(id, r#"{"op":"finish"}"#, None);
        for _ in 0..256 {
            assert_eq!(contexts.execute(id, &source, None), json!(0x9118));
        }
        contexts.complete_task();
        assert_eq!(contexts.execute(id, &source, None), json!(0x9119));
        let deletion = json!({"op":"deleteSync","i":[fence]}).to_string();
        contexts.execute(id, &deletion, None);
        assert_eq!(contexts.execute(id, &source, None), Value::Null);
        assert_eq!(
            contexts.execute(id, r#"{"op":"getError"}"#, None),
            json!(0x0502)
        );
        contexts.remove(id);
        assert_eq!(contexts.execute(id, &source, None), json!({"lost":true}));
    }
}
