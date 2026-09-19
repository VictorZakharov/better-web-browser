use super::*;

#[test]
fn deferred_front_cannot_be_overtaken_but_async_defer_is_an_async_element() {
    let page = Page::parse_scripted(
        "<script defer src=/first.js></script><script defer src=/second.js></script><script async defer src=/fast.js></script>",
        "https://example.test/",
    );
    let mut queue = ParserScripts::new(&page.scripts);
    queue.complete(&script_resource(&page.scripts[1]), Some("second()"));
    assert!(!queue.has_ready());
    assert!(queue.deferred_pending());
    queue.complete(&script_resource(&page.scripts[2]), Some("fast()"));
    assert_eq!(
        queue.pop_ready().unwrap().node.id(),
        page.scripts[2].node.id()
    );
    assert!(!queue.has_ready());
    queue.complete(&script_resource(&page.scripts[0]), None);
    assert!(queue.pop_ready().unwrap().code.is_none());
    assert_eq!(
        queue.pop_ready().unwrap().node.id(),
        page.scripts[1].node.id()
    );
    assert!(!queue.is_pending());
}

#[test]
fn inline_classic_defer_is_ignored_and_inline_async_module_joins_async_set() {
    let page = Page::parse_scripted(
        "<script defer>classic()</script><script type=module>module()</script><script async type=module>asyncModule()</script>",
        "https://example.test/",
    );
    assert!(page.scripts[0].blocks_first_paint);
    assert!(!page.scripts[0].executes_after_parsing);
    let queue = ParserScripts::new(&page.scripts);
    assert_eq!(queue.deferred.len(), 1);
    assert_eq!(queue.ready.len(), 1);
    assert_eq!(queue.ready[0].node.id(), page.scripts[2].node.id());
}

fn scripts() -> (Page, ParserScripts) {
    let page = Page::parse_scripted(
        "<script async src=/slow.js></script><script async src=/fast.js></script><script async src=/fast.js></script>",
        "https://example.test/",
    );
    let queue = ParserScripts::new(&page.scripts);
    (page, queue)
}

#[test]
fn ready_elements_do_not_wait_for_an_earlier_fetch_and_shared_urls_execute_twice() {
    let (page, mut queue) = scripts();
    let fast = script_resource(&page.scripts[1]);
    assert!(
        !queue.has_ready(),
        "pending network work must not request clock polling"
    );
    queue.complete(&fast, Some("fast()"));
    for index in [1, 2] {
        let next = queue.ready.pop_front().unwrap();
        assert_eq!(next.node.id(), page.scripts[index].node.id());
        assert_eq!(next.code.as_deref(), Some("fast()"));
    }
    assert!(!queue.has_ready());
    queue.complete(&fast, Some("fast()"));
    assert!(
        !queue.has_ready(),
        "a duplicate response must not replay scripts"
    );
    queue.complete(&script_resource(&page.scripts[0]), Some("slow()"));
    assert_eq!(
        queue.ready.pop_front().unwrap().node.id(),
        page.scripts[0].node.id()
    );
    assert!(queue.waiting.is_empty());
}

#[test]
fn failed_fetch_completes_each_owner_once_without_suppressing_success() {
    let (page, mut queue) = scripts();
    queue.complete(&script_resource(&page.scripts[1]), None);
    queue.complete(&script_resource(&page.scripts[0]), Some("slow()"));
    assert!(queue.ready.pop_front().unwrap().code.is_none());
    assert!(queue.ready.pop_front().unwrap().code.is_none());
    assert_eq!(
        queue.ready.pop_front().unwrap().code.as_deref(),
        Some("slow()")
    );
    assert!(!queue.has_ready());
}

#[test]
fn preparation_survives_detachment_and_src_changes() {
    let (page, mut queue) = scripts();
    let element = &page.scripts[1].node;
    crate::engine::dom::Node::remove_from_parent(element);
    element.set_attr("src", "/replacement.js");
    queue.complete(&script_resource(&page.scripts[1]), Some("original()"));
    let prepared = queue.ready.pop_front().unwrap();
    assert_eq!(prepared.node.id(), element.id());
    assert_eq!(prepared.source_url, "https://example.test/fast.js");
    assert_eq!(prepared.code.as_deref(), Some("original()"));
}

#[test]
fn fetching_contract_includes_credentials_not_just_url() {
    let page = Page::parse_scripted(
        "<script async crossorigin=anonymous src=/shared.js></script><script async crossorigin=use-credentials src=/shared.js></script>",
        "https://example.test/",
    );
    let mut queue = ParserScripts::new(&page.scripts);
    queue.complete(&script_resource(&page.scripts[1]), Some("credentialed()"));
    assert_eq!(
        queue.ready.pop_front().unwrap().node.id(),
        page.scripts[1].node.id()
    );
    assert_eq!(queue.waiting.len(), 1);
}
