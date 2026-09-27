use super::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn rows(value: &str) -> ReadSnapshot {
    vec![(value.into(), "value".into(), "detail".into())].into()
}
fn poll_until(worker: &mut SnapshotWorker, ready: impl Fn(&SnapshotWorker) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        worker.poll();
        if ready(worker) {
            return;
        }
        assert!(Instant::now() < deadline, "worker did not complete");
        thread::yield_now();
    }
}
#[test]
fn filtering_never_reads_and_failures_retain_snapshot() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&count);
    let mut worker = SnapshotWorker::new(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(rows("saved"))
        } else {
            Err("read refused".into())
        }
    });
    worker.request(ReadRequest::new(Resource::History));
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Ready
        )
    });
    for filter in ["", "s", "sa", "saved", "none"]
        .into_iter()
        .cycle()
        .take(100)
    {
        worker.snapshot(Resource::History).unwrap().filtered(filter);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    worker.request(ReadRequest::new(Resource::History));
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Stale(_)
        )
    });
    assert_eq!(
        worker.snapshot(Resource::History).unwrap().filtered("")[1].0,
        "saved"
    );
}
#[test]
fn coalesces_pending_resources_and_discards_obsolete_and_pre_mutation_reads() {
    let (started, observe) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut worker = SnapshotWorker::new(move |request| {
        started.send(request.resource).unwrap();
        blocked.recv().unwrap();
        Ok(rows(request.video.as_ref().map_or("old", |(root, _)| root)))
    });
    worker.request(ReadRequest::new(Resource::Overview));
    assert_eq!(
        observe.recv_timeout(Duration::from_secs(3)).unwrap(),
        Resource::Overview
    );
    for value in ["one", "two", "latest"] {
        worker.request(ReadRequest {
            resource: Resource::Video,
            video: Some((value.into(), frametime_domain::video::VideoGoal::RawLatency)),
        });
    }
    release.send(()).unwrap();
    assert_eq!(
        observe.recv_timeout(Duration::from_secs(3)).unwrap(),
        Resource::Video
    );
    worker.invalidate(); // Includes queued completion and active video read.
    worker.request(ReadRequest::new(Resource::History));
    release.send(()).unwrap();
    assert_eq!(
        observe.recv_timeout(Duration::from_secs(3)).unwrap(),
        Resource::History
    );
    worker.poll();
    assert!(worker.snapshot(Resource::Overview).unwrap().rows.is_none());
    assert!(worker.snapshot(Resource::Video).unwrap().rows.is_none());
    release.send(()).unwrap();
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Ready
        )
    });
    assert!(observe.try_recv().is_err());
}
#[test]
fn same_resource_obsolete_result_cannot_replace_latest_request() {
    let (started, observe) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let mut worker = SnapshotWorker::new(move |request| {
        started.send(()).unwrap();
        blocked.recv().unwrap();
        Ok(rows(&request.video.unwrap().0))
    });
    for value in ["old", "new"] {
        worker.request(ReadRequest {
            resource: Resource::Video,
            video: Some((value.into(), frametime_domain::video::VideoGoal::RawLatency)),
        });
        if value == "old" {
            observe.recv_timeout(Duration::from_secs(3)).unwrap();
        }
    }
    release.send(()).unwrap();
    observe.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.poll();
    assert!(worker.snapshot(Resource::Video).unwrap().rows.is_none());
    release.send(()).unwrap();
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::Video).unwrap().state,
            SnapshotState::Ready
        )
    });
    assert_eq!(
        worker.snapshot(Resource::Video).unwrap().filtered("")[1].0,
        "new"
    );
}
#[test]
fn closing_during_read_drops_pending_work_without_waiting() {
    let (started, observe) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let (finished, done) = mpsc::channel();
    let mut worker = SnapshotWorker::new(move |_| {
        started.send(()).unwrap();
        blocked.recv().unwrap();
        finished.send(()).unwrap();
        Ok(ReadSnapshot::default())
    });
    worker.request(ReadRequest::new(Resource::Overview));
    observe.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.request(ReadRequest::new(Resource::History));
    drop(worker);
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(observe.recv_timeout(Duration::from_secs(3)).is_err());
}
#[test]
fn empty_and_initial_failure_are_explicit() {
    let mut worker = SnapshotWorker::new(|r| {
        if r.resource == Resource::History {
            Ok(ReadSnapshot::default())
        } else {
            Err("denied".into())
        }
    });
    worker.request(ReadRequest::new(Resource::History));
    worker.request(ReadRequest::new(Resource::Recovery));
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Empty
        ) && matches!(
            w.snapshot(Resource::Recovery).unwrap().state,
            SnapshotState::Failed(_)
        )
    });
}

#[test]
fn metadata_stale_and_filtered_out_rows_cannot_select_recovery() {
    let mut snapshot = Snapshot {
        state: SnapshotState::Ready,
        rows: Some(ReadSnapshot {
            recovery_keys: BTreeMap::from([(0, "saved step".into())]),
            ..rows("saved step")
        }),
        generation: 1,
    };
    assert_eq!(snapshot.selected_data_key(0, ""), None);
    assert_eq!(
        snapshot.selected_data_key(1, "saved"),
        Some("saved step".into())
    );
    assert_eq!(snapshot.selected_data_key(1, "absent"), None);
    assert_eq!(snapshot.selected_data_key(2, ""), None);
    for state in [
        SnapshotState::Loading,
        SnapshotState::Stale("denied".into()),
        SnapshotState::Failed("denied".into()),
        SnapshotState::Empty,
    ] {
        snapshot.state = state;
        assert_eq!(snapshot.selected_data_key(1, ""), None);
    }
}

#[test]
fn worker_panic_reports_failure_and_can_refresh_again() {
    let mut first = true;
    let mut worker = SnapshotWorker::new(move |_| {
        if first {
            first = false;
            panic!("injected reader failure");
        }
        Ok(rows("recovered"))
    });
    worker.request(ReadRequest::new(Resource::History));
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Failed(_)
        )
    });
    worker.request(ReadRequest::new(Resource::History));
    poll_until(&mut worker, |w| {
        matches!(
            w.snapshot(Resource::History).unwrap().state,
            SnapshotState::Ready
        )
    });
}
#[test]
fn display_text_without_an_explicit_recovery_key_is_not_actionable() {
    let snapshot = Snapshot {
        state: SnapshotState::Ready,
        rows: Some(rows("Unknown recovery record")),
        generation: 1,
    };
    assert_eq!(snapshot.selected_data_key(1, ""), None);
}
