use std::path::PathBuf;

const DEFAULT_SETTLE_MS: u64 = 1_500;
const DEFAULT_TIMEOUT_MS: u64 = 20_000;

pub(crate) struct Cli {
    pub(crate) wpt_root: PathBuf,
    pub(crate) manifest: PathBuf,
    pub(crate) browser: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) settle_ms: u64,
    pub(crate) timeout_ms: u64,
    pub(crate) jobs: usize,
    pub(crate) filter: Option<String>,
}

impl Cli {
    pub(crate) fn parse() -> Result<Option<Self>, String> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut wpt_root = std::env::var_os("BREEZE_WPT_ROOT").map(PathBuf::from);
        let mut manifest = repo_root.join("tests/wpt/manifest.json");
        let executable = if cfg!(windows) {
            "better-web-browser.exe"
        } else {
            "better-web-browser"
        };
        let mut browser = repo_root.join("target/release").join(executable);
        let mut output = repo_root.join("target/wpt/report.json");
        let mut settle_ms = DEFAULT_SETTLE_MS;
        let mut timeout_ms = DEFAULT_TIMEOUT_MS;
        let mut jobs = 1_usize;
        let mut filter = None;
        let mut arguments = std::env::args().skip(1);

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--wpt-root" => wpt_root = Some(path_value(&mut arguments, "--wpt-root")?),
                "--manifest" => manifest = path_value(&mut arguments, "--manifest")?,
                "--browser" => browser = path_value(&mut arguments, "--browser")?,
                "--output" => output = path_value(&mut arguments, "--output")?,
                "--settle-ms" => {
                    settle_ms = number_value(&mut arguments, "--settle-ms")?;
                }
                "--timeout-ms" => {
                    timeout_ms = number_value(&mut arguments, "--timeout-ms")?;
                }
                "--jobs" => {
                    jobs = usize::try_from(number_value(&mut arguments, "--jobs")?)
                        .map_err(|_| "--jobs is too large".to_string())?;
                }
                "--filter" => filter = Some(text_value(&mut arguments, "--filter")?),
                "--help" | "-h" => {
                    println!("{}", Self::usage());
                    return Ok(None);
                }
                option => return Err(format!("unknown option: {option}\n\n{}", Self::usage())),
            }
        }

        let wpt_root = wpt_root.ok_or_else(|| {
            format!(
                "--wpt-root is required (or set BREEZE_WPT_ROOT)\n\n{}",
                Self::usage()
            )
        })?;
        if !(100..=60_000).contains(&settle_ms) {
            return Err("--settle-ms must be between 100 and 60000".to_string());
        }
        if timeout_ms <= settle_ms {
            return Err("--timeout-ms must be greater than --settle-ms".to_string());
        }
        if !(1..=16).contains(&jobs) {
            return Err("--jobs must be between 1 and 16".to_string());
        }

        Ok(Some(Self {
            wpt_root,
            manifest,
            browser,
            output,
            settle_ms,
            timeout_ms,
            jobs,
            filter,
        }))
    }

    fn usage() -> &'static str {
        "Usage: wpt-runner --wpt-root <checkout> [options]\n\
         \n\
         Options:\n\
           --manifest <file>     Curated manifest (default: tests/wpt/manifest.json)\n\
           --browser <file>      Release Breeze executable\n\
           --output <file>       Machine-readable JSON report\n\
           --settle-ms <number>  Hidden-browser settle period (default: 1500)\n\
           --timeout-ms <number> Per-test process timeout (default: 20000)\n\
           --jobs <number>       Concurrent hidden browser processes (default: 1)\n\
           --filter <text>       Run cases whose path or area contains text\n\
           -h, --help            Show this help"
    }
}

fn text_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn path_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<PathBuf, String> {
    text_value(arguments, option).map(PathBuf::from)
}

fn number_value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<u64, String> {
    text_value(arguments, option)?
        .parse()
        .map_err(|_| format!("{option} requires a positive integer"))
}
