use super::StateStore;

#[test]
fn place_id_round_trips() {
    let store = StateStore::open_in_memory().unwrap();
    assert_eq!(store.place_id().unwrap(), None);
    store.set_place_id(123_456).unwrap();
    assert_eq!(store.place_id().unwrap(), Some(123_456));
}

#[test]
fn clearing_the_place_id_unbinds_the_project() {
    let store = StateStore::open_in_memory().unwrap();
    store.set_place_id(123_456).unwrap();
    store.clear_place_id().unwrap();
    assert_eq!(store.place_id().unwrap(), None);
}
