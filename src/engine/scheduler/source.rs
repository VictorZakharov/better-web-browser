/// Specification-defined task sources share a scheduler but retain FIFO order.
/// Font completion is a task, not a promise job or a cancelable author timer.
/// <https://drafts.csswg.org/css-font-loading-3/#task-sources>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskSource {
    Timer,
    IdleTask,
    Networking,
    MediaElement,
    FontLoading,
    PerformanceTimeline,
    UserInteraction,
    Lifecycle,
    DomManipulation,
    Rendering,
}
