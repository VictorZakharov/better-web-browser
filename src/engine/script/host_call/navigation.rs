//! Navigation effects and cancellable HTML form-navigation tasks.
use super::*;
use crate::navigation::request::NavigationOptions;

#[derive(Default)]
pub(in crate::engine::script) struct FormPlans {
    next: u32,
    plans: HashMap<u32, Plan>,
}
struct Plan {
    token: u32,
    url: String,
    options: NavigationOptions,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "navigate" => {
            let resolved = state.resolved_url(&argument_string(args, 1)?);
            state.navigation_url = Some(resolved.clone());
            // HTML Location-object navigate: a pre-load redirect without activation
            // replaces; a loaded document's assign/href navigation pushes history.
            state.navigation_options = NavigationOptions {
                replace_history: args.get(2).is_some_and(JsValue::to_boolean)
                    || (!state.document_load.complete() && !state.user_input_active),
                user_initiated: state.user_input_active,
                ..Default::default()
            };
            Ok(Some(js_string(resolved)))
        }
        "navigateRequest" | "planFormNavigation" => {
            let value = argument_string(args, 1)?;
            let serialized = argument_string(args, 2)?;
            let mut options: NavigationOptions =
                serde_json::from_str(&serialized).map_err(|error| {
                    JsNativeError::typ().with_message(format!("invalid navigation: {error}"))
                })?;
            options
                .validate()
                .map_err(|error| JsNativeError::range().with_message(error))?;
            let resolved = state.resolved_url(&value);
            crate::navigation::ParsedUrl::parse(&resolved)
                .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
            if operation == "planFormNavigation" {
                if !state.policy.allows_url("form-action", &resolved, 0) {
                    state.diagnose("form navigation blocked by Content Security Policy".into());
                    return Ok(Some(JsValue::from(0u32)));
                }
                if state.sandbox.forms_blocked {
                    state.diagnose("form navigation blocked by the document sandbox".into());
                    return Ok(Some(JsValue::from(0u32)));
                }
                let id = argument_id(args, 3);
                let _form = state
                    .nodes
                    .get(&id)
                    .filter(|node| node.tag_name() == Some("form"))
                    .cloned()
                    .ok_or_else(|| {
                        JsNativeError::typ().with_message("invalid form navigation owner")
                    })?;
                // Per-form replacement models the planned-navigation task. Bound retained
                // bodies independently of the DOM budget while author script is running.
                if state.form_plans.plans.len() >= 32 && !state.form_plans.plans.contains_key(&id) {
                    return Err(JsNativeError::range()
                        .with_message("too many pending form navigations")
                        .into());
                }
                state.form_plans.next = state.form_plans.next.checked_add(1).ok_or_else(|| {
                    JsNativeError::range().with_message("form navigation tokens exhausted")
                })?;
                let token = state.form_plans.next;
                options.user_initiated = state.user_input_active;
                options.form_submission = true;
                state.form_plans.plans.insert(
                    id,
                    Plan {
                        token,
                        url: resolved,
                        options,
                    },
                );
                return Ok(Some(JsValue::from(token)));
            }
            state.navigation_url = Some(resolved.clone());
            state.navigation_options = options;
            Ok(Some(js_string(resolved)))
        }
        "commitFormNavigation" => {
            let id = argument_id(args, 1);
            let token = argument_id(args, 2);
            if state
                .form_plans
                .plans
                .get(&id)
                .is_some_and(|plan| plan.token == token)
            {
                let plan = state.form_plans.plans.remove(&id).unwrap();
                state.navigation_url = Some(plan.url);
                state.navigation_options = plan.options;
            }
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
