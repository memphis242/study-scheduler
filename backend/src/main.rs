use backend::db::open_database;

fn main() -> anyhow::Result<()> {
    let path =
        std::env::var("STUDY_SCHEDULER_DB").unwrap_or_else(|_| "study-scheduler.db".to_string());
    open_database(path)?;
    println!("Study Scheduler database initialized.");
    Ok(())
}
