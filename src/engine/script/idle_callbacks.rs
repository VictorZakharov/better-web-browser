//! Document-owned cooperative idle periods, separate from the ordinary timer queue.
//! https://w3c.github.io/requestidlecallback/#processing
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::{Duration, Instant};

const MAX_IDLE_PERIOD: Duration = Duration::from_millis(50);

struct Period {
    id: u32,
    deadline: Duration,
    started: Instant,
    budget: Duration,
}

#[derive(Default)]
pub(super) struct IdleCallbacks {
    requests: HashMap<u32, Option<Duration>>,
    timeouts: BTreeSet<(Duration, u32)>,
    pending: VecDeque<u32>,
    runnable: VecDeque<u32>,
    period: Option<Period>,
    next_period: u32,
    not_before: Duration,
}

pub(super) struct IdleInvocation {
    pub(super) id: u32,
    pub(super) period: u32,
    pub(super) timed_out: bool,
}

pub(super) enum Callback {
    Timer(u32),
    Idle(IdleInvocation),
}

impl Callback {
    pub(super) fn id(&self) -> u32 {
        match self {
            Self::Timer(id) => *id,
            Self::Idle(task) => task.id,
        }
    }
    pub(super) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle(_))
    }
}

impl IdleCallbacks {
    pub(super) fn schedule(&mut self, id: u32, now: Duration, timeout: Duration) {
        self.cancel(id);
        let due = (!timeout.is_zero()).then(|| now.saturating_add(timeout));
        self.requests.insert(id, due);
        if let Some(due) = due {
            self.timeouts.insert((due, id));
        }
        self.pending.push_back(id);
    }

    pub(super) fn cancel(&mut self, id: u32) {
        if let Some(due) = self.requests.remove(&id) {
            if let Some(due) = due {
                self.timeouts.remove(&(due, id));
            }
            self.pending.retain(|pending| *pending != id);
            self.runnable.retain(|pending| *pending != id);
        }
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn next_due(&self, now: Duration, blocked: bool) -> Option<Duration> {
        let timeout = self.timeouts.first().map(|(due, _)| *due);
        if blocked || self.requests.is_empty() {
            return timeout;
        }
        let idle = if let Some(period) = &self.period {
            if self.runnable.is_empty() {
                period.deadline.max(now)
            } else {
                now
            }
        } else {
            self.not_before.max(now)
        };
        Some(timeout.map_or(idle, |due| due.min(idle)))
    }

    pub(super) fn take_timeout(&mut self, now: Duration) -> Option<IdleInvocation> {
        let (due, id) = *self.timeouts.first()?;
        if due > now {
            return None;
        }
        self.cancel(id);
        Some(IdleInvocation {
            id,
            period: 0,
            timed_out: true,
        })
    }

    pub(super) fn interrupt(&mut self) {
        if let Some(period) = self.period.take() {
            // An interrupted period cannot restart before its previous deadline.
            self.not_before = self.not_before.max(period.deadline);
        }
    }

    pub(super) fn take_idle(
        &mut self,
        now: Duration,
        next_task: Option<Duration>,
    ) -> Option<IdleInvocation> {
        if self.period.as_ref().is_some_and(|period| {
            now >= period.deadline || period.started.elapsed() >= period.budget
        }) {
            self.interrupt();
        }
        if self.period.is_none() {
            if now < self.not_before || self.requests.is_empty() {
                return None;
            }
            let budget = next_task.map_or(MAX_IDLE_PERIOD, |due| {
                due.saturating_sub(now).min(MAX_IDLE_PERIOD)
            });
            let budget = self
                .timeouts
                .first()
                .map_or(budget, |(due, _)| due.saturating_sub(now).min(budget));
            if budget.is_zero() {
                return None;
            }
            self.next_period = self.next_period.wrapping_add(1).max(1);
            self.period = Some(Period {
                id: self.next_period,
                deadline: now.saturating_add(budget),
                started: Instant::now(),
                budget,
            });
            // Reposts stay pending until a new idle period. Older runnable work stays first.
            self.runnable.append(&mut self.pending);
        }
        let id = self.runnable.pop_front()?;
        if let Some(Some(due)) = self.requests.remove(&id) {
            self.timeouts.remove(&(due, id));
        }
        Some(IdleInvocation {
            id,
            period: self.period.as_ref()?.id,
            timed_out: false,
        })
    }

    pub(super) fn remaining(
        &self,
        id: u32,
        now: Duration,
        next_task: Option<Duration>,
        blocked: bool,
    ) -> f64 {
        let Some(period) = self.period.as_ref().filter(|period| period.id == id) else {
            return 0.0;
        };
        if blocked {
            return 0.0;
        }
        let deadline = next_task.map_or(period.deadline, |due| due.min(period.deadline));
        let remaining = deadline
            .saturating_sub(now)
            .min(period.budget.saturating_sub(period.started.elapsed()));
        // No finer resolution than the current millisecond Performance clock. Never round up.
        remaining.as_millis() as f64
    }
}

#[cfg(test)]
mod tests;
