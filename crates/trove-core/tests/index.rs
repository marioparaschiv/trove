use trove_core::{data_dir::DataDir, index::Index};

#[test]
fn opening_an_empty_data_folder_creates_the_schema_and_reopening_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = DataDir::at(temp.path().join("data")).unwrap();

    let index = Index::open(&data_dir).unwrap();
    assert_eq!(index.schema_version().unwrap(), 1);
    let device_id = index.device_id().unwrap();
    drop(index);

    let reopened = Index::open(&data_dir).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), 1);
    assert_eq!(reopened.device_id().unwrap(), device_id);
}
