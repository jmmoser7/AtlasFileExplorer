//! Shared link worker capacity, ordering, and admission.

use super::*;

#[test]
fn shared_link_workers_preserve_capacity_fifo_and_yield_to_other_sources() {
    let ready = worker::ReadyQueue::default();
    let first = AgentLink::with_scheduler(ready.clone());
    let other = AgentLink::with_scheduler(ready.clone());
    let workspace = temp_workspace("shared_link_fifo");
    let dir = workspace.join("link");
    let request = |index| {
        LinkWork::Request(
            dir.clone(),
            Box::new(AgentRequest {
                id: format!("request-{index}"),
                ..Default::default()
            }),
        )
    };
    for index in 0..8 {
        first.tx.try_send(request(index)).unwrap();
    }
    assert!(first.tx.try_send(request(8)).is_err());
    other
        .tx
        .try_send(LinkWork::Request(
            workspace.join("other"),
            Box::new(AgentRequest {
                id: "other-source".into(),
                ..Default::default()
            }),
        ))
        .unwrap();
    assert_eq!(ready.len(), 2, "one ready token per source");

    for index in 0..8 {
        let token = ready.try_take(false).unwrap();
        assert!(std::sync::Arc::ptr_eq(&token, &first.tx));
        token.run_next();
        let saved: AgentRequest =
            serde_json::from_slice(&std::fs::read(dir.join("request.json")).unwrap()).unwrap();
        assert_eq!(saved.id, format!("request-{index}"));
        if index == 0 {
            let token = ready.try_take(false).unwrap();
            assert!(std::sync::Arc::ptr_eq(&token, &other.tx));
            token.run_next();
        }
    }
    assert_eq!(ready.len(), 0);
    first.tx.try_send(request(8)).unwrap();
    ready.try_take(false).unwrap().run_next();
    assert_eq!(ready.len(), 0);
    let _ = std::fs::remove_dir_all(workspace);
}

#[test]
fn reserved_worker_takes_promoted_link_in_fifo_order_while_ordinary_workers_are_occupied() {
    let ready = worker::ReadyQueue::default();
    let cold: Vec<_> = (0..4)
        .map(|_| AgentLink::with_scheduler(ready.clone()))
        .collect();
    for link in &cold {
        link.tx.try_send(LinkWork::Roots(Vec::new())).unwrap();
    }
    let _held: Vec<_> = (0..3).map(|_| ready.try_take(false).unwrap()).collect();
    assert!(
        ready.try_take(true).is_none(),
        "ordinary work cannot occupy the reserve"
    );

    let workspace = temp_workspace("foreground-reserve");
    let dir = workspace.join("link");
    let mut active = AgentLink::with_scheduler(ready.clone());
    assert!(active.tick_write_context_in(&dir, &context()));
    active
        .send_request_in(
            &dir,
            &AgentRequest {
                id: "local-send".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ready.len(), 2, "promotion moves the existing token");
    let token = ready.try_take(true).unwrap();
    assert!(std::sync::Arc::ptr_eq(&token, &active.tx));
    token.run_next();
    assert!(dir.join("context.json").is_file());
    assert!(
        !dir.join("request.json").exists(),
        "accepted earlier work stays first"
    );
    ready.try_take(true).unwrap().run_next();
    assert!(dir.join("request.json").is_file());
    assert!(ready.try_take(true).is_none());
    assert!(std::sync::Arc::ptr_eq(
        &ready.try_take(false).unwrap(),
        &cold[3].tx
    ));
    let _ = std::fs::remove_dir_all(workspace);
}
