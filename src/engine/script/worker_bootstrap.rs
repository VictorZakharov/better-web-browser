pub(super) const WORKER_BOOTSTRAP: &str = concat!(
    include_str!("bootstrap/dom_exception.js"),
    include_str!("bootstrap/worker_base.js"),
    include_str!("bootstrap/streams.js"),
    include_str!("bootstrap/network_data.js"),
    include_str!("bootstrap/network_body.js"),
    include_str!("bootstrap/network_types.js"),
    include_str!("bootstrap/network_fetch.js"),
    include_str!("bootstrap/network_xhr.js"),
    include_str!("bootstrap/structured_clone.js"),
    include_str!("bootstrap/worker_scope.js"),
);
