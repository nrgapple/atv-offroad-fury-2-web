//! Durable local release queue. Only the worker mutates upstream or production.
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    owner: PathBuf,
    queue: PathBuf,
    controller: PathBuf,
    target: PathBuf,
    playwright: PathBuf,
    url: String,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Job {
    id: String,
    source: String,
    status: String,
    stage: String,
    integrated: Option<String>,
    error: Option<String>,
}
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
fn atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}-{}", std::process::id(), now()));
    let mut file = File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn lock(path: &Path) -> Result<File> {
    Ok(File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}
fn output(cwd: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
fn run(cwd: &Path, program: &str, args: &[&str], log: &File, config: &Config) -> Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?)
        .env("CARGO_TARGET_DIR", &config.target)
        .env("PLAYWRIGHT_MODULE", &config.playwright)
        .env("ATV_TEST_URL", &config.url);
    writeln!(&*log, "\n$ {program} {}", args.join(" "))?;
    if !command.status()?.success() {
        return Err(format!("{program} {} failed; see job log", args.join(" ")).into());
    }
    Ok(())
}
fn text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}
fn common() -> Result<PathBuf> {
    Ok(PathBuf::from(output(
        Path::new("."),
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?))
}
fn config() -> Result<Config> {
    read(&common()?.join("atv-release.json"))
}
fn jobs(config: &Config) -> Result<Vec<Job>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(config.queue.join("jobs"))? {
        let entry = entry?;
        let path = entry.path().join("job.json");
        if path.exists() && !entry.file_name().to_string_lossy().starts_with('.') {
            result.push(read::<Job>(&path)?);
        }
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
fn save(config: &Config, job: &Job) -> Result<()> {
    atomic(
        &config.queue.join("jobs").join(&job.id).join("job.json"),
        job,
    )
}
fn step(config: &Config, job: &mut Job, stage: &str) -> Result<()> {
    job.stage = stage.into();
    save(config, job)
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone())
}
fn submit(config: &Config, args: &[String]) -> Result<()> {
    let root = PathBuf::from(output(
        Path::new("."),
        "git",
        &["rev-parse", "--show-toplevel"],
    )?);
    if !output(
        &root,
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err("Commit your changes first; submission requires a clean worktree".into());
    }
    let commit = output(&root, "git", &["rev-parse", "HEAD"])?;
    // Snapshot before enqueueing. The submit lock serializes baseline reads and receipts.
    let submit_lock = lock(&config.queue.join("submit.lock"))?;
    submit_lock.lock()?;
    for job in jobs(config)? {
        if job.source == commit && job.status != "cancelled" {
            println!("{}", serde_json::to_string(&job)?);
            return Ok(());
        }
    }
    let id = format!("{:020}-{}", now(), &commit[..12]);
    let staging = config.queue.join("jobs").join(format!(".{id}"));
    fs::create_dir_all(&staging)?;
    let log = File::create(staging.join("submit.log"))?;
    let content = option(args, "--content")
        .map(PathBuf::from)
        .unwrap_or(config.queue.join("published/content"));
    let images = option(args, "--images")
        .map(PathBuf::from)
        .unwrap_or(root.join("private/reports/devlog-images"));
    let mut helper = vec![
        text(&config.controller).to_owned(),
        "snapshot".into(),
        "--source".into(),
        text(&root).into(),
        "--commit".into(),
        commit.clone(),
        "--job".into(),
        text(&staging).into(),
        "--queue".into(),
        text(&config.queue).into(),
        "--content".into(),
        text(&content).into(),
        "--images".into(),
        text(&images).into(),
    ];
    if args.iter().any(|a| a == "--reviewed-images") {
        helper.push("--reviewed-images".into());
    }
    let references: Vec<&str> = helper.iter().map(String::as_str).collect();
    if let Err(error) = run(&root, "python3", &references, &log, config) {
        return Err(format!("{error}: {}", staging.join("submit.log").display()).into());
    }
    output(
        &root,
        "git",
        &[
            "update-ref",
            &format!("refs/atv-release/jobs/{id}"),
            &commit,
        ],
    )?;
    let job = Job {
        id: id.clone(),
        source: commit,
        status: "queued".into(),
        stage: "submitted".into(),
        integrated: None,
        error: None,
    };
    atomic(&staging.join("job.json"), &job)?;
    fs::rename(&staging, config.queue.join("jobs").join(&id))?;
    File::open(config.queue.join("jobs"))?.sync_all()?;
    println!("{}", serde_json::to_string(&job)?);
    drop(submit_lock);
    if args.iter().any(|a| a == "--wait") {
        wait(config, &id)?;
    }
    Ok(())
}
fn wait(config: &Config, id: &str) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err("Invalid job ID".into());
    }
    loop {
        let job: Job = read(&config.queue.join("jobs").join(id).join("job.json"))?;
        if job.status == "complete" {
            println!("{}", serde_json::to_string(&job)?);
            return Ok(());
        }
        if job.status == "failed" {
            return Err(format!(
                "Job {id} failed at {}: {}",
                job.stage,
                job.error.unwrap_or_default()
            )
            .into());
        }
        thread::sleep(Duration::from_secs(2));
    }
}
fn process(config: &Config, job: &mut Job) -> Result<()> {
    let directory = config.queue.join("jobs").join(&job.id);
    let checkout = directory.join("checkout");
    let log = File::options()
        .append(true)
        .create(true)
        .open(directory.join("worker.log"))?;
    run(
        &config.owner,
        "git",
        &["fetch", "origin", "main"],
        &log,
        config,
    )?;
    let upstream = output(&config.owner, "git", &["rev-parse", "origin/main"])?;
    let pushed = matches!(
        job.stage.as_str(),
        "pushed" | "uploading" | "uploaded" | "deploying" | "deployed" | "verifying" | "verified"
    );
    if pushed && job.integrated.as_deref() != Some(upstream.as_str()) {
        return Err(
            "Upstream main changed after this job pushed; queue halted for reconciliation".into(),
        );
    }
    if !checkout.exists() {
        run(
            &config.owner,
            "git",
            &["worktree", "add", "--detach", text(&checkout), &upstream],
            &log,
            config,
        )?;
    }
    if !pushed {
        step(config, job, "integrating")?;
        run(
            &checkout,
            "git",
            &["reset", "--hard", &upstream],
            &log,
            config,
        )?;
        run(
            &checkout,
            "python3",
            &[
                text(&config.controller),
                "integrate",
                "--commit",
                &job.source,
            ],
            &log,
            config,
        )?;
        let commit = output(&checkout, "git", &["rev-parse", "HEAD"])?;
        job.integrated = Some(commit);
        save(config, job)?;
    }
    let commit = job.integrated.clone().ok_or("Missing integrated commit")?;
    // Re-running a crashed job rebuilds the same committed revision, never a dirty source.
    run(
        &checkout,
        "git",
        &["reset", "--hard", &commit],
        &log,
        config,
    )?;
    let submit_lock = lock(&config.queue.join("submit.lock"))?;
    submit_lock.lock()?;
    run(
        &checkout,
        "python3",
        &[
            text(&config.controller),
            "prepare",
            "--job",
            text(&directory),
            "--queue",
            text(&config.queue),
        ],
        &log,
        config,
    )?;
    drop(submit_lock);
    if !pushed {
        step(config, job, "checking")?;
    }
    run(
        &checkout,
        "python3",
        &["tools/verify_repository.py"],
        &log,
        config,
    )?;
    run(&checkout, "make", &["check"], &log, config)?;
    run(
        &checkout,
        "python3",
        &[
            "-m",
            "unittest",
            "discover",
            "-s",
            "tools",
            "-p",
            "test_release*.py",
        ],
        &log,
        config,
    )?;
    run(
        &checkout,
        "node",
        &["--test", "cloudflare/worker.test.mjs"],
        &log,
        config,
    )?;
    run(&checkout, "make", &["web", "relay"], &log, config)?;
    let release = checkout.join("work/release");
    run(
        &checkout,
        "python3",
        &[
            "tools/package_release.py",
            "--commit",
            &commit,
            "--release-scoped",
            "--release-scoped",
        ],
        &log,
        config,
    )?;
    if !pushed {
        step(config, job, "built")?;
        // Normal push rejects a concurrent update. Never force main.
        run(
            &checkout,
            "git",
            &["push", "origin", &format!("{commit}:refs/heads/main")],
            &log,
            config,
        )?;
        step(config, job, "pushed")?;
    }
    let current = output(
        &checkout,
        "git",
        &["ls-remote", "origin", "refs/heads/main"],
    )?;
    if current.split_whitespace().next() != Some(&commit) {
        return Err("Upstream main changed; refusing publication".into());
    }
    step(config, job, "uploading")?;
    run(
        &checkout,
        "npx",
        &["--yes", "wrangler@4.149.0", "whoami"],
        &log,
        config,
    )?;
    run(
        &checkout,
        "python3",
        &[
            "tools/upload_cloudflare_content.py",
            "--release",
            text(&release),
            "--commit",
            &commit,
            "--release-scoped",
        ],
        &log,
        config,
    )?;
    step(config, job, "uploaded")?;
    run(
        &checkout,
        "python3",
        &[
            "tools/stage_cloudflare.py",
            "--release",
            text(&release),
            "--commit",
            &commit,
            "--release-scoped",
        ],
        &log,
        config,
    )?;
    let current = output(
        &checkout,
        "git",
        &["ls-remote", "origin", "refs/heads/main"],
    )?;
    if current.split_whitespace().next() != Some(&commit) {
        return Err("Upstream main changed after upload; refusing deployment".into());
    }
    step(config, job, "deploying")?;
    run(
        &checkout,
        "npx",
        &[
            "--yes",
            "wrangler@4.149.0",
            "deploy",
            "--config",
            "work/cloudflare/wrangler.jsonc",
        ],
        &log,
        config,
    )?;
    step(config, job, "deployed")?;
    step(config, job, "verifying")?;
    run(
        &checkout,
        "python3",
        &[
            "tools/verify_cloudflare.py",
            "--url",
            &config.url,
            "--commit",
            &commit,
            "--release",
            text(&release),
        ],
        &log,
        config,
    )?;
    run(
        &checkout,
        "node",
        &["tools/test_frontend.mjs"],
        &log,
        config,
    )?;
    run(&checkout, "node", &["tools/test_devlog.mjs"], &log, config)?;
    step(config, job, "verified")?;
    let submit_lock = lock(&config.queue.join("submit.lock"))?;
    submit_lock.lock()?;
    run(
        &checkout,
        "python3",
        &[
            text(&config.controller),
            "promote",
            "--queue",
            text(&config.queue),
        ],
        &log,
        config,
    )?;
    job.status = "complete".into();
    job.error = None;
    save(config, job)?;
    Ok(())
}
fn next_job(jobs: &[Job]) -> Option<&Job> {
    // A failed release that may have pushed blocks later publication until retried.
    if jobs.iter().any(|j| {
        j.status == "failed"
            && matches!(
                j.stage.as_str(),
                "built"
                    | "pushed"
                    | "uploading"
                    | "uploaded"
                    | "deploying"
                    | "deployed"
                    | "verifying"
                    | "verified"
            )
    }) {
        return None;
    }
    jobs.iter()
        .find(|j| j.status == "queued" || j.status == "running")
}
fn worker(config: &Config, once: bool) -> Result<()> {
    let worker_lock = lock(&config.queue.join("worker.lock"))?;
    match worker_lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    loop {
        for mut job in jobs(config)? {
            let marker = config.queue.join("jobs").join(&job.id).join("retry");
            if marker.exists() && job.status == "failed" {
                job.status = "queued".into();
                job.error = None;
                save(config, &job)?;
                fs::remove_file(marker)?;
            }
        }
        let available = jobs(config)?;
        if let Some(mut job) = next_job(&available).cloned() {
            job.status = "running".into();
            job.error = None;
            save(config, &job)?;
            if let Err(error) = process(config, &mut job) {
                job.status = "failed".into();
                job.error = Some(error.to_string());
                save(config, &job)?;
            }
        } else if once {
            break;
        } else {
            thread::sleep(Duration::from_secs(2));
        }
    }
    Ok(())
}
fn retry(config: &Config, id: &str) -> Result<()> {
    if !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') || id.is_empty() {
        return Err("Invalid job ID".into());
    }
    let mut job: Job = read(&config.queue.join("jobs").join(id).join("job.json"))?;
    if job.status != "failed" {
        return Err("Only failed jobs can be retried".into());
    }
    job.error = None;
    atomic(&config.queue.join("jobs").join(id).join("retry"), &true)?;
    println!("Retry requested for {id}");
    Ok(())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn home_cache_target() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("HOME")?).join("Library/Caches/atv-release-target"))
}
fn install(args: &[String]) -> Result<()> {
    let git = common()?;
    let owner = git
        .parent()
        .ok_or("Expected non-bare project repository")?
        .to_path_buf();
    let remote = output(&owner, "git", &["remote", "get-url", "origin"])?;
    if ![
        "git@github.com:nrgapple/atv-offroad-fury-2-web.git",
        "https://github.com/nrgapple/atv-offroad-fury-2-web.git",
    ]
    .contains(&remote.as_str())
    {
        return Err("Expected the project's GitHub origin remote".into());
    }
    if output(
        &owner,
        "gh",
        &[
            "api",
            "repos/nrgapple/atv-offroad-fury-2-web",
            "--jq",
            ".private",
        ],
    )? != "true"
    {
        return Err("Source repository must remain private".into());
    }
    let source = PathBuf::from(output(
        Path::new("."),
        "git",
        &["rev-parse", "--show-toplevel"],
    )?);
    let queue = PathBuf::from(
        option(args, "--storage").ok_or("install requires --storage <private local directory>")?,
    );
    if !queue.is_absolute() {
        return Err("Storage must be an absolute path".into());
    }
    let playwright = PathBuf::from(
        option(args, "--playwright")
            .ok_or("install requires --playwright <absolute Playwright module path>")?,
    );
    if !playwright.is_absolute() || !playwright.exists() {
        return Err("Playwright module path must exist".into());
    }
    fs::create_dir_all(queue.join("jobs"))?;
    let controller_dir = queue.join("controller");
    fs::create_dir_all(&controller_dir)?;
    let controller = controller_dir.join("release_resources.py");
    for name in ["release_resources.py", "package_content.py"] {
        fs::copy(source.join("tools").join(name), controller_dir.join(name))?;
    }
    let config = Config {
        owner: owner.clone(),
        queue: fs::canonicalize(&queue)?,
        controller,
        target: option(args, "--target")
            .map(PathBuf::from)
            .unwrap_or(home_cache_target()?),
        playwright,
        url: "https://atv-offroad-fury-2.champtoday.workers.dev".into(),
    };
    let log = File::create(queue.join("install.log"))?;
    if !queue.join("published").exists() {
        run(
            &owner,
            "python3",
            &[
                text(&config.controller),
                "initialize",
                "--queue",
                text(&queue),
            ],
            &log,
            &config,
        )?;
    }
    atomic(&git.join("atv-release.json"), &config)?;
    let home = PathBuf::from(env::var("HOME")?);
    let binary = home.join(".local/bin/atv-release");
    fs::create_dir_all(binary.parent().unwrap())?;
    let temporary = binary.with_extension("new");
    fs::copy(env::current_exe()?, &temporary)?;
    fs::rename(temporary, &binary)?;
    let plist = home.join("Library/LaunchAgents/com.champtoday.atv-release.plist");
    fs::create_dir_all(plist.parent().unwrap())?;
    let path = format!(
        "{}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
        text(&home)
    );
    let body = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>com.champtoday.atv-release</string><key>ProgramArguments</key><array><string>{}</string><string>worker</string></array><key>WorkingDirectory</key><string>{}</string><key>EnvironmentVariables</key><dict><key>PATH</key><string>{}</string></dict><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>10</integer><key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string></dict></plist>",
        xml(text(&binary)),
        xml(text(&owner)),
        xml(&path),
        xml(text(&queue.join("service.log"))),
        xml(text(&queue.join("service.log")))
    );
    fs::write(&plist, body)?;
    println!(
        "Installed {} and {}. Start with launchctl bootstrap gui/$(id -u) {}",
        binary.display(),
        plist.display(),
        plist.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("atv-release: {error}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("install") => install(&args),
        Some("submit") => submit(&config()?, &args),
        Some("worker") => worker(&config()?, args.iter().any(|a| a == "--once")),
        Some("status") => {
            println!("{}", serde_json::to_string_pretty(&jobs(&config()?)?)?);
            Ok(())
        }
        Some("wait") => wait(&config()?, args.get(1).ok_or("wait requires job ID")?),
        Some("retry") => retry(&config()?, args.get(1).ok_or("retry requires job ID")?),
        _ => {
            println!(
                "atv-release submit [--content DIRECTORY] [--images DIRECTORY --reviewed-images]\natv-release status\natv-release wait JOB_ID\natv-release worker [--once]\natv-release retry JOB_ID\natv-release install --storage DIRECTORY --playwright MODULE"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job(id: &str, status: &str, stage: &str) -> Job {
        Job {
            id: id.into(),
            source: "a".repeat(40),
            status: status.into(),
            stage: stage.into(),
            integrated: None,
            error: None,
        }
    }
    #[test]
    fn recovery_prioritizes_interrupted_job_and_blocks_after_push() {
        let list = vec![
            job("1", "running", "pushed"),
            job("2", "queued", "submitted"),
        ];
        assert_eq!(next_job(&list).unwrap().id, "1");
        let list = vec![
            job("1", "failed", "uploading"),
            job("2", "queued", "submitted"),
        ];
        assert!(next_job(&list).is_none());
        let list = vec![
            job("1", "failed", "checking"),
            job("2", "queued", "submitted"),
        ];
        assert_eq!(next_job(&list).unwrap().id, "2");
    }
    #[test]
    fn kernel_lock_excludes_another_worker_and_recovers_on_drop() {
        let path = env::temp_dir().join(format!("atv-release-lock-{}", now()));
        let first = lock(&path).unwrap();
        first.try_lock().unwrap();
        let second = lock(&path).unwrap();
        assert!(matches!(
            second.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(first);
        second.try_lock().unwrap();
        drop(second);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn atomic_receipt_survives_readback() {
        let path = env::temp_dir().join(format!("atv-release-job-{}", now()));
        atomic(&path, &job("1", "queued", "submitted")).unwrap();
        assert_eq!(read::<Job>(&path).unwrap().status, "queued");
        fs::remove_file(path).unwrap();
    }
}
