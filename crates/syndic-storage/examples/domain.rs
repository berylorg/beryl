use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{CreateThread, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join("beryl-syndic-domain-example");
    std::fs::create_dir_all(&path)?;
    let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
    let syndic = SyndicStorage::register(&mut home)?;
    let creation = CreateThread::ordinary(
        SyndicThreadId::from_bytes([1; 16]),
        SyndicDraftId::from_bytes([2; 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([3; 16]),
            RootId::from_bytes([4; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                "C:\\beryl-syndic-example",
            )?,
        ),
        SyndicTimestamp::from_unix_millis(1),
    );
    let mut command = HomeCommand::new(home.home_revision()?);
    command.add(syndic.create_thread(syndic.revision(&home)?, creation))?;
    home.execute(command)?;
    let current = syndic
        .current_draft(
            &home,
            SyndicThreadId::from_bytes([1; 16]),
            SyndicPointReadLimit::new(400_000)?,
        )?
        .expect("created thread has a current draft");
    println!("Draft revision: {}", current.draft().revision().get());
    let text = syndic
        .sealed_content_text_range(&home, current.draft().content(), 0, 16_384)?
        .expect("created draft content exists");
    assert_eq!(text.text(), "");
    assert_eq!(text.next_offset(), None);
    home.close()?;
    Ok(())
}
