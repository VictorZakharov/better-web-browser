use super::*;
use std::{
    sync::{
        Condvar,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[test]
fn nonblocking_link_progress_leaves_room_for_all_three_native_executable_jobs() {
    let pending = Pending::default();
    assert!(pending.link_capacity());
    *pending.count.lock().unwrap() = QUEUED_TASKS - 3;
    assert!(pending.link_capacity());
    *pending.count.lock().unwrap() = QUEUED_TASKS - 2;
    assert!(!pending.link_capacity());
    *pending.count.lock().unwrap() = QUEUED_TASKS + WORKERS;
    assert!(!pending.link_capacity());
    *pending.count.lock().unwrap() = 0;
    assert!(pending.link_capacity());
}

#[derive(Default)]
struct Gate {
    ready: Mutex<bool>,
    wake: Condvar,
}

impl Gate {
    fn release(&self) {
        *self.ready.lock().unwrap() = true;
        self.wake.notify_all();
    }

    fn wait(&self) {
        let ready = self.ready.lock().unwrap();
        drop(self.wake.wait_while(ready, |ready| !*ready).unwrap());
    }
}

struct Fixture {
    id: usize,
    gate: Arc<Gate>,
    started: mpsc::Sender<usize>,
    finished: mpsc::Sender<usize>,
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
}

unsafe extern "C" fn run_fixture(data: *mut c_void) {
    // Test-owned equivalent of ANGLE's move-only heap closure and event signal.
    let fixture = unsafe { Box::from_raw(data.cast::<Fixture>()) };
    let active = fixture.active.fetch_add(1, Ordering::SeqCst) + 1;
    fixture.maximum.fetch_max(active, Ordering::SeqCst);
    let _ = fixture.started.send(fixture.id);
    fixture.gate.wait();
    fixture.active.fetch_sub(1, Ordering::SeqCst);
    let _ = fixture.finished.send(fixture.id);
}

struct Fixtures {
    gate: Arc<Gate>,
    start_send: mpsc::Sender<usize>,
    started: mpsc::Receiver<usize>,
    finish_send: mpsc::Sender<usize>,
    finished: mpsc::Receiver<usize>,
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
}

impl Fixtures {
    fn new() -> Self {
        let (start_send, started) = mpsc::channel();
        let (finish_send, finished) = mpsc::channel();
        Self {
            gate: Arc::default(),
            start_send,
            started,
            finish_send,
            finished,
            active: Arc::default(),
            maximum: Arc::default(),
        }
    }

    fn task(&self, id: usize) -> Task {
        Task {
            callback: run_fixture,
            data: Box::into_raw(Box::new(Fixture {
                id,
                gate: self.gate.clone(),
                started: self.start_send.clone(),
                finished: self.finish_send.clone(),
                active: self.active.clone(),
                maximum: self.maximum.clone(),
            }))
            .cast(),
        }
    }

    fn started(&self, count: usize) {
        for _ in 0..count {
            self.started.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }

    fn finish(&self, count: usize) -> Vec<usize> {
        self.gate.release();
        let mut completed = (0..count)
            .map(|_| self.finished.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect::<Vec<_>>();
        completed.sort_unstable();
        completed
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        // Even a failed assertion cannot leave a blocked native-style closure.
        self.gate.release();
    }
}

#[test]
fn submissions_return_while_native_closures_remain_pending() {
    let fixtures = Fixtures::new();
    let pool = Pool::new(2, 4).unwrap();
    pool.post(fixtures.task(0));
    pool.post(fixtures.task(1));
    fixtures.started(2);
    for id in 2..6 {
        pool.post(fixtures.task(id));
    }
    // No timing threshold: native work cannot finish until this test releases it.
    assert_eq!(fixtures.active.load(Ordering::SeqCst), 2);
    assert!(fixtures.finished.try_recv().is_err());
    assert_eq!(fixtures.finish(6), [0, 1, 2, 3, 4, 5]);
    assert_eq!(fixtures.maximum.load(Ordering::SeqCst), 2);
}

#[test]
fn fixed_worker_limit_and_bounded_queue_apply_backpressure_without_dropping_tasks() {
    let fixtures = Fixtures::new();
    let pool = Arc::new(Pool::new(WORKERS, 2).unwrap());
    for id in 0..WORKERS {
        pool.post(fixtures.task(id));
    }
    fixtures.started(WORKERS);
    pool.post(fixtures.task(4));
    pool.post(fixtures.task(5));
    let pending = fixtures.task(6);
    let (begin_send, begin_receive) = mpsc::channel();
    let (posted_send, posted_receive) = mpsc::channel();
    let producer_pool = pool.clone();
    let producer = thread::spawn(move || {
        begin_send.send(()).unwrap();
        producer_pool.post(pending);
        posted_send.send(()).unwrap();
    });
    begin_receive.recv_timeout(Duration::from_secs(5)).unwrap();
    let blocked = posted_receive
        .recv_timeout(Duration::from_millis(30))
        .is_err();
    let completed = fixtures.finish(7);
    producer.join().unwrap();
    assert!(blocked, "a full compiler queue must not grow without bound");
    assert_eq!(completed, [0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(fixtures.maximum.load(Ordering::SeqCst), WORKERS);
}

#[test]
fn disconnected_mailbox_executes_the_native_event_once_instead_of_leaking_it() {
    let fixtures = Fixtures::new();
    fixtures.gate.release();
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    Pool {
        sender,
        pending: Arc::default(),
    }
    .post(fixtures.task(0));
    assert_eq!(fixtures.finish(1), [0]);
    assert!(fixtures.finished.try_recv().is_err());
}

#[test]
fn closing_the_pool_drains_already_owned_native_closures() {
    let fixtures = Fixtures::new();
    let pool = Pool::new(1, 3).unwrap();
    pool.post(fixtures.task(0));
    fixtures.started(1);
    for id in 1..4 {
        pool.post(fixtures.task(id));
    }
    drop(pool);
    assert_eq!(fixtures.finish(4), [0, 1, 2, 3]);
    assert_eq!(fixtures.maximum.load(Ordering::SeqCst), 1);
}

#[test]
fn display_quiescence_covers_running_and_queued_native_work() {
    let fixtures = Fixtures::new();
    let pool = Pool::new(1, 3).unwrap();
    for id in 0..3 {
        pool.post(fixtures.task(id));
    }
    fixtures.started(1);
    assert_eq!(*pool.pending.count.lock().unwrap(), 3);
    let pending = pool.pending.clone();
    let (started_send, started_receive) = mpsc::channel();
    let (idle_send, idle_receive) = mpsc::channel();
    let waiter = thread::spawn(move || {
        started_send.send(()).unwrap();
        pending.wait();
        idle_send.send(()).unwrap();
    });
    started_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let remained_busy = idle_receive
        .recv_timeout(Duration::from_millis(30))
        .is_err();
    let completed = fixtures.finish(3);
    waiter.join().unwrap();
    assert!(remained_busy);
    assert_eq!(completed, [0, 1, 2]);
    assert_eq!(*pool.pending.count.lock().unwrap(), 0);
    pool.pending.wait();
}
