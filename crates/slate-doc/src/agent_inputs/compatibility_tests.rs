use crate::scene::PortalNode;

#[test]
fn old_wires_and_portals_deserialize_without_new_fields() {
    let p = PortalNode::unbound_agent("legacy", "cursor");
    let mut value = serde_json::to_value(&p).unwrap();
    for field in ["bundle", "seed", "view"] {
        value["agent"].as_object_mut().unwrap().remove(field);
    }
    let read: PortalNode = serde_json::from_value(value).unwrap();
    assert_eq!(read.agent.unwrap().view, atlas_agent::PortalView::Chat);
}
