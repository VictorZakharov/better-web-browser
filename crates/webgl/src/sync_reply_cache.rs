//! Task-stable fence polls can reuse a successful native reply on the caller.
//! This cache never samples completion or lets script advance a GPU task.
use super::Command;
use serde_json::Value;
use std::collections::HashMap;

const MAX_REPLIES: usize = 256;

#[derive(Default)]
pub(super) struct Cache {
    replies: HashMap<u32, HashMap<String, Value>>,
    finished: std::collections::HashSet<u32>,
}

impl Cache {
    pub(super) fn get(&self, context: u32, command: &str) -> Option<Value> {
        if self.finished.contains(&context)
            && serde_json::from_str::<Command>(command).is_ok_and(|c| {
                c.op == "finish" && c.i.is_empty() && c.f.is_empty() && c.text.is_empty()
            })
        {
            return Some(Value::Null);
        }
        self.replies.get(&context)?.get(command).cloned()
    }

    pub(super) fn record(&mut self, context: u32, source: &str, value: &Value) {
        // Most commands mutate state and cannot be cached. Avoid reparsing their
        // JSON on this caller-side fast path: the native owner still validates
        // every command. This filter only excludes cache candidates; it never
        // admits a reply without the structured validation below.
        if source.len() > 256
            || ![
                "\"getQueryParameter\"",
                "\"finish\"",
                "\"getSyncParameter\"",
                "\"clientWaitSync\"",
            ]
            .iter()
            .any(|name| source.contains(name))
        {
            self.remove(context);
            return;
        }
        // Retain only successful task-frozen status and zero-flag, zero-time
        // waits. FLUSH_COMMANDS_BIT has a real submission side effect and must
        // always reach ANGLE; invalid/deleted/foreign names never populate us.
        let command = serde_json::from_str::<Command>(source).ok();
        let cacheable = command
            .as_ref()
            .is_some_and(|command| match command.op.as_str() {
                "getQueryParameter" => {
                    command.i.len() == 2 && command.i[1] == 0x8867 && value.is_boolean()
                }
                "finish" => {
                    command.i.is_empty()
                        && command.f.is_empty()
                        && command.text.is_empty()
                        && value.is_null()
                }
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
            });
        if !cacheable {
            self.remove(context);
            return;
        }
        if command.is_some_and(|c| c.op == "finish") {
            // The first finish reached ANGLE and drained all earlier work.
            // Repeating it before another non-poll command has no additional
            // work to drain. Task boundaries still discard this knowledge.
            self.finished.insert(context);
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
        self.finished.remove(&context);
    }

    pub(super) fn clear(&mut self) {
        self.replies.clear();
        self.finished.clear();
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
            (r#"{"op":"finish"}"#.into(), Value::Null, true),
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
    fn completed_finish_is_reused_only_until_new_work_or_a_task_boundary() {
        let mut cache = Cache::default();
        let finish = r#"{"op":"finish"}"#;
        let query = r#"{"op":"getQueryParameter","i":[1,34919]}"#;
        assert_eq!(cache.get(7, finish), None);
        cache.record(7, finish, &Value::Null);
        cache.record(7, query, &json!(false));
        for _ in 0..256 {
            assert_eq!(cache.get(7, finish), Some(Value::Null));
            assert_eq!(cache.get(7, query), Some(json!(false)));
        }
        assert_eq!(cache.get(8, finish), None);
        cache.record(7, r#"{"op":"drawArrays","i":[4,0,3]}"#, &Value::Null);
        assert_eq!(cache.get(7, finish), None);
        assert_eq!(cache.get(7, query), None);
        cache.record(7, finish, &Value::Null);
        cache.clear();
        assert_eq!(cache.get(7, finish), None);
        cache.record(7, finish, &json!({"lost":true}));
        assert_eq!(cache.get(7, finish), None);
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

    #[test]
    fn session_query_poll_and_finish_reuse_preserves_native_publication_and_reuse() {
        let mut contexts = super::super::session::Contexts::default();
        let id = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
        let query = contexts
            .execute(id, r#"{"op":"createQuery"}"#, None)
            .as_u64()
            .unwrap();
        let begin = json!({"op":"beginQuery","i":[0x8c2f,query]}).to_string();
        let end = r#"{"op":"endQuery","i":[35887]}"#;
        let available = json!({"op":"getQueryParameter","i":[query,0x8867]}).to_string();
        contexts.execute(id, &begin, None);
        contexts.execute(id, end, None);
        for _ in 0..256 {
            assert_eq!(
                contexts.execute(id, r#"{"op":"finish"}"#, None),
                Value::Null
            );
            assert_eq!(contexts.execute(id, &available, None), json!(false));
        }
        contexts.complete_task();
        assert_eq!(contexts.execute(id, &available, None), json!(true));
        contexts.execute(id, &begin, None);
        assert_eq!(contexts.execute(id, &available, None), Value::Null);
        assert_eq!(
            contexts.execute(id, r#"{"op":"getError"}"#, None),
            json!(0x0502)
        );
        contexts.execute(id, end, None);
        assert_eq!(contexts.execute(id, &available, None), json!(false));
        let delete = json!({"op":"deleteQuery","i":[query]}).to_string();
        contexts.execute(id, &delete, None);
        assert_eq!(contexts.execute(id, &available, None), Value::Null);
        assert_eq!(
            contexts.execute(id, r#"{"op":"getError"}"#, None),
            json!(0x0502)
        );
    }
}
