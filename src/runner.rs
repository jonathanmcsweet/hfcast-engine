//! Drives a chosen `voacapl` binary.
//!
//! Concurrent runs need a whole `itshfbc` tree each, not just unique input and
//! output filenames. `decred.for` builds its antenna scratch filename from the
//! antenna index alone:
//!
//! ```text
//!   write(gainfile,'(4hgain,i2.2,4h.dat)') iantr
//! ```
//!
//! so every run writes `<root>/run/gain01.dat` and `gain02.dat` under those
//! fixed names. Two runs sharing a root therefore overwrite each other's gain
//! files, and a run that reads one mid-write dies with a Fortran end-of-file
//! fault. Unique deck names do not help, because these names come from the
//! engine rather than from the caller.
//!
//! The tree is about 1.4 MB, so each run gets a private copy. That is far
//! cheaper than the alternative of serialising every prediction.
//!
//! A run that fails is reported, not propagated. Whether a given build of the
//! engine completes at all is one of the things being measured.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// A single run is given far longer than it needs; the limit exists so a hung
/// process cannot stall the whole sweep.
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// How often a running child is checked for completion.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub fn itshfbc_dir() -> PathBuf {
    env::var_os("HFCAST_ITSHFBC")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("itshfbc"))
}

pub fn variants_dir() -> PathBuf {
    env::var_os("PROPCORE_VARIANTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("workspace/vendor/voacapl-variants"))
}

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Path to the binary `build-variants.sh` produced for a variant.
pub fn variant_bin(name: &str) -> PathBuf {
    variants_dir().join(name).join("src/voacapw/voacapl")
}

#[derive(Debug)]
pub enum RunError {
    Io(io::Error),
    /// The engine ran and exited non-zero. The message is what it printed,
    /// which for a Fortran runtime fault is the traceback.
    Failed {
        code: Option<i32>,
        output: String,
    },
    TimedOut,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Io(e) => write!(f, "io error: {e}"),
            RunError::Failed { code, output } => {
                let first = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                match code {
                    Some(c) => write!(f, "exit {c}: {first}"),
                    None => write!(f, "killed by signal: {first}"),
                }
            }
            RunError::TimedOut => write!(f, "timed out"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<io::Error> for RunError {
    fn from(e: io::Error) -> Self {
        RunError::Io(e)
    }
}

/// A private copy of the `itshfbc` tree, removed when it goes out of scope.
///
/// This is what makes concurrent runs safe. See the module comment for why
/// unique deck filenames are not enough on their own.
pub struct IsolatedRoot {
    path: PathBuf,
}

impl IsolatedRoot {
    /// Copies the shared tree to a private location keyed by `tag` and
    /// the process id.
    ///
    /// The process id matters: harness binaries name their trees after
    /// the case, and two of them running the same corpus at once would
    /// otherwise pick the same directory — where the first act of
    /// `create` is to delete it. That does not fail; it silently
    /// truncates one run's reference output and reports the missing
    /// cells as differences.
    pub fn create(tag: &str) -> io::Result<Self> {
        let base = env::var_os("PROPCORE_SCRATCH")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let path = base.join(format!("propcore-itshfbc-{}-{tag}", std::process::id()));
        // A leftover from an interrupted run would otherwise be reused.
        let _ = fs::remove_dir_all(&path);
        copy_dir_all(&itshfbc_dir(), &path)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replaces one file in the private tree with the given bytes.
    ///
    /// A stock tree links whole directories (`coeffs` especially) into the
    /// read-only share directory, so the parent directory is first turned
    /// into a real directory of per-entry links; only then can one entry be
    /// swapped without touching the shared installation.
    pub fn replace_file(&self, relative: &str, bytes: &[u8]) -> io::Result<()> {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            materialize_dir(parent)?;
        }
        let _ = fs::remove_file(&target);
        fs::write(&target, bytes)
    }
}

/// If `dir` is a symlink to a directory, replaces it with a real directory
/// containing a symlink per entry of the original target.
fn materialize_dir(dir: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(dir)?;
    if !meta.file_type().is_symlink() {
        return Ok(());
    }
    let mut target = fs::read_link(dir)?;
    if target.is_relative() {
        if let Some(parent) = dir.parent() {
            target = parent.join(target);
        }
    }
    fs::remove_file(dir)?;
    fs::create_dir(dir)?;
    for entry in fs::read_dir(&target)? {
        let entry = entry?;
        std::os::unix::fs::symlink(entry.path(), dir.join(entry.file_name()))?;
    }
    Ok(())
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Copies a tree, keeping symbolic links as links rather than following them.
///
/// A stock `itshfbc` is largely a symlink farm pointing into the installed
/// share directory — `coeffs` alone is the bulk of the data. Those targets are
/// read-only, so linking to them keeps a private tree small; only the writable
/// `run` directory is really duplicated. Following them instead would copy tens
/// of megabytes per run.
fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            let mut target = fs::read_link(&from)?;
            // The copy sits elsewhere in the filesystem, so a relative target
            // would point somewhere different once recreated.
            if target.is_relative() {
                if let Some(parent) = from.parent() {
                    target = parent.join(target);
                }
            }
            std::os::unix::fs::symlink(target, &to)?;
        } else if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Runs one deck inside `root` and returns the listing.
///
/// `root` must not be shared with a concurrently running call.
pub fn run_deck(bin: &Path, root: &Path, deck: &str) -> Result<String, RunError> {
    run_deck_with_env(bin, root, deck, &[])
}

/// Like [`run_deck`], with extra environment variables for the engine.
///
/// The trace variant reads `PROPCORE_TRACE` and dumps stage intermediates
/// into the directory it names; the stock engine ignores it.
pub fn run_deck_with_env(
    bin: &Path,
    root: &Path,
    deck: &str,
    env: &[(&str, &str)],
) -> Result<String, RunError> {
    let run_dir = root.join("run");
    let input_name = "propcore.dat";
    let output_name = "propcore.out";

    fs::write(run_dir.join(input_name), deck)?;
    run_to_completion(bin, root, input_name, output_name, env)?;
    fs::read_to_string(run_dir.join(output_name)).map_err(RunError::from)
}

/// Runs the reference in area-coverage mode and returns the grid file
/// it writes.
///
/// An area run reads none of the card decks the other methods use.
/// `voacapl` takes the literal argument `area`, a mode word (`calc`),
/// and the name of a keyed text file under the tree's `areadata`
/// directory; the results go to a sibling file with a `.vg1`
/// extension. `name` is that file's stem, `voa` its contents.
pub fn run_area(
    bin: &Path,
    itshfbc: &Path,
    name: &str,
    voa: &str,
    inverse: bool,
) -> Result<String, RunError> {
    // Inverse coverage is its own invocation word and its own directory:
    // the input goes under `area_inv` rather than `areadata`.
    let (dir, mode) = if inverse {
        (itshfbc.join("area_inv").join("default"), "inv")
    } else {
        (itshfbc.join("areadata").join("default"), "area")
    };
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{name}.voa")), voa)?;
    let out = dir.join(format!("{name}.vg1"));
    let _ = fs::remove_file(&out);
    let mut command = Command::new(bin);
    let mut child = command
        .arg(itshfbc)
        .arg(mode)
        .arg("calc")
        .arg(format!("default/{name}.voa"))
        .current_dir(itshfbc)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                let o = child.wait_with_output()?;
                return Err(RunError::Failed {
                    code: status.code(),
                    output: String::from_utf8_lossy(&o.stderr).into_owned(),
                });
            }
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err(RunError::TimedOut);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(fs::read_to_string(&out)?)
}

fn run_to_completion(
    bin: &Path,
    itshfbc: &Path,
    input_name: &str,
    output_name: &str,
    env: &[(&str, &str)],
) -> Result<(), RunError> {
    let mut command = Command::new(bin);
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command
        .arg(itshfbc)
        .arg(input_name)
        .arg(output_name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(status) => {
                if status.success() {
                    return Ok(());
                }
                let out = child.wait_with_output()?;
                let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&out.stdout).into_owned();
                }
                return Err(RunError::Failed {
                    code: status.code(),
                    output: text,
                });
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunError::TimedOut);
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Applies `f` to every item with at most `limit` running at once, preserving
/// input order in the results.
pub fn map_limit<T, R, F>(items: &[T], limit: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T, usize) -> R + Sync,
{
    let slots: Vec<Mutex<Option<R>>> = (0..items.len()).map(|_| Mutex::new(None)).collect();
    let cursor = AtomicUsize::new(0);
    let workers = limit.clamp(1, items.len().max(1));

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = cursor.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { return };
                let value = f(item, index);
                *slots[index].lock().expect("worker slot poisoned") = Some(value);
            });
        }
    });

    slots
        .into_iter()
        .map(|slot| {
            slot.into_inner()
                .expect("worker slot poisoned")
                .expect("every slot is filled before the scope ends")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_limit_preserves_order() {
        let items: Vec<usize> = (0..50).collect();
        let doubled = map_limit(&items, 8, |n, _| n * 2);
        assert_eq!(doubled, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn map_limit_handles_an_empty_input() {
        let empty: Vec<usize> = Vec::new();
        assert!(map_limit(&empty, 4, |n, _| *n).is_empty());
    }

    #[test]
    fn map_limit_passes_the_index() {
        let items = vec!['a', 'b', 'c'];
        assert_eq!(map_limit(&items, 2, |_, i| i), vec![0, 1, 2]);
    }

    #[test]
    fn copy_dir_all_reproduces_a_nested_tree() {
        let base = env::temp_dir().join("propcore-copy-test");
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(src.join("run")).expect("create source");
        fs::write(src.join("run/gain01.dat"), "scratch").expect("write nested");
        fs::write(src.join("top.txt"), "top").expect("write top");

        copy_dir_all(&src, &dst).expect("copy");

        assert_eq!(
            fs::read_to_string(dst.join("run/gain01.dat")).expect("nested copy"),
            "scratch"
        );
        assert_eq!(
            fs::read_to_string(dst.join("top.txt")).expect("top copy"),
            "top"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn copy_dir_all_keeps_symlinks_as_links() {
        // A stock itshfbc links `coeffs` into the share directory. Following
        // the link would copy the whole coefficient set for every run.
        let base = env::temp_dir().join("propcore-symlink-test");
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(src.join("real")).expect("create source");
        fs::write(src.join("real/data.txt"), "shared").expect("write target");
        std::os::unix::fs::symlink(src.join("real"), src.join("linked")).expect("link");

        copy_dir_all(&src, &dst).expect("copy");

        assert!(
            fs::symlink_metadata(dst.join("linked"))
                .expect("linked entry")
                .file_type()
                .is_symlink(),
            "the link was dereferenced instead of recreated"
        );
        assert_eq!(
            fs::read_to_string(dst.join("linked/data.txt")).expect("through link"),
            "shared"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn isolated_roots_do_not_share_a_path() {
        // Two concurrent runs must not land in the same tree, or they overwrite
        // each other's gain01.dat.
        let a = IsolatedRoot::create("test-a").expect("root a");
        let b = IsolatedRoot::create("test-b").expect("root b");
        assert_ne!(a.path(), b.path());
        assert!(a.path().join("run").is_dir());
        assert!(b.path().join("run").is_dir());
    }

    #[test]
    fn replace_file_materialises_a_linked_directory_without_touching_the_share() {
        // Simulate a stock tree: root/coeffs is a symlink to a shared dir.
        let base = env::temp_dir().join("propcore-replace-test");
        let share = base.join("share");
        let root_dir = base.join("root");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&share).expect("share");
        fs::create_dir_all(&root_dir).expect("root");
        fs::write(share.join("fof2CCIR.daw"), b"climatology").expect("shared file");
        fs::write(share.join("coeff01w.bin"), b"other").expect("other file");
        std::os::unix::fs::symlink(&share, root_dir.join("coeffs")).expect("link");

        let root = IsolatedRoot {
            path: root_dir.clone(),
        };
        root.replace_file("coeffs/fof2CCIR.daw", b"irtam")
            .expect("replace");

        assert_eq!(
            fs::read(root_dir.join("coeffs/fof2CCIR.daw")).expect("patched"),
            b"irtam"
        );
        // The neighbours still resolve through their links.
        assert_eq!(
            fs::read(root_dir.join("coeffs/coeff01w.bin")).expect("neighbour"),
            b"other"
        );
        // The shared installation is untouched.
        assert_eq!(
            fs::read(share.join("fof2CCIR.daw")).expect("shared"),
            b"climatology"
        );
        // Forget the root so Drop does not delete the whole test base twice.
        std::mem::forget(root);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn dropping_an_isolated_root_removes_it() {
        let path = {
            let root = IsolatedRoot::create("test-drop").expect("root");
            root.path().to_path_buf()
        };
        assert!(!path.exists());
    }
}
