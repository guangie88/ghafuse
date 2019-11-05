mod github;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory,
    ReplyEntry, Request,
};
use libc::ENOENT;
use snafu::{ErrorCompat, ResultExt, Snafu};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use structopt::StructOpt;

use crate::github::{Credentials, GitHub};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Need mount path as first argument"))]
    MissingMountPoint,

    #[snafu(display("Invalid mount for path {}: {}", mountpoint.display(), source))]
    InvalidMount {
        mountpoint: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, StructOpt)]
#[structopt(name = "ghafuse", about = "Options for ghafuse")]
struct Opt {
    /// Mount path to mount GitHub releases listing on
    #[structopt(parse(from_os_str))]
    mount_path: PathBuf,

    /// Repository owner to target
    #[structopt()]
    owner: String,

    /// Repository name to target
    #[structopt()]
    repo: String,

    /// Username to add as part of credentials
    #[structopt(short = "u")]
    username: Option<String>,

    /// Password to add as part of credentials
    #[structopt(short = "p")]
    password: Option<String>,
}

const TTL: Duration = Duration::from_secs(1); // 1 second
const HELLO_TXT_CONTENT: &str = "HelloWorld";

const HELLO_DIR_ATTR: FileAttr = FileAttr {
    ino: 1,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH, // 1970-01-01 00:00:00
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

fn create_dir_attr(ino: u64) -> FileAttr {
    FileAttr {
        ino,
        size: 0,
        blocks: 1,
        atime: UNIX_EPOCH, // 1970-01-01 00:00:00
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::Directory,
        perm: 0o755,
        nlink: 2,
        uid: 501,
        gid: 20,
        rdev: 0,
        flags: 0,
    }
}

fn create_file_attr(ino: u64) -> FileAttr {
    FileAttr {
        ino,
        size: 13,
        blocks: 1,
        atime: UNIX_EPOCH, // 1970-01-01 00:00:00
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 501,
        gid: 20,
        rdev: 0,
        flags: 0,
    }
}

struct GhaFs {
    state: GitHub,
    owner: String,
    repo: String,
}

impl GhaFs {
    fn new(state: GitHub, owner: String, repo: String) -> GhaFs {
        GhaFs { state, owner, repo }
    }
}

impl Filesystem for GhaFs {
    fn lookup(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        reply: ReplyEntry,
    ) {
        // Only called when `ls` in mounted dir
        println!(
            "lookup, parent: {}, name: {}",
            parent,
            name.to_string_lossy()
        );

        let releases = self
            .state
            .releases(&self.owner, &self.repo)
            .expect("lookup.releases GET error");

        if parent == 1 {
            let name = name.to_str().expect("lookup.name.to_str error");
            let find_res = releases
                .into_iter()
                .enumerate()
                .find(|(_, r)| name == &r.tag_name);

            match find_res {
                Some((idx, _)) => {
                    // println!("{} has index {}", name, idx);
                    reply.entry(&TTL, &create_dir_attr(idx as u64 + 2), 0);
                }
                _ => reply.error(ENOENT),
            }
        } else {
            // reply.error(ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        reply: ReplyData,
    ) {
        // Triggered on `cat` command on the file
        println!("read, ino: {}", ino);

        if ino != 1 {
            let content = format!("{}-{}\n", HELLO_TXT_CONTENT, ino);
            reply.data(&content.as_bytes()[offset as usize..]);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        // keeps getting called with `readdir`
        println!("getattr, ino: {}", ino);

        match ino {
            1 => reply.attr(&TTL, &HELLO_DIR_ATTR),
            x @ _ => reply.attr(&TTL, &create_file_attr(x)),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        // keeps getting called with `getattr`
        // does get called when in subdir with the entered dir's inode
        // This should be the function to fully traverse the tags -> assets
        // so that all the inodes can be allocated and be saved as state
        // . should always point to its own ino
        // .. should always point to parent, which is always 1 for GitHub case
        // but .. should point to the original mount dir's parent
        println!("readdir, ino: {}", ino);

        let releases = self
            .state
            .releases(&self.owner, &self.repo)
            .expect("readdir.releases GET error");

        // Root has ino 1
        let tags = if ino == 1 {
            releases
                .into_iter()
                .enumerate()
                .map(|(idx, r)| {
                    ((idx + 2) as u64, FileType::Directory, r.tag_name)
                })
                .collect()
        } else {
            vec![]
        };

        let entries = vec![
            (ino, FileType::Directory, ".".to_owned()),
            (1, FileType::Directory, "..".to_owned()),
        ]
        .into_iter()
        .chain(tags.into_iter());

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize)
        {
            // i + 1 means the index of the next entry
            reply.add(entry.0, (i + 1) as i64, entry.1, entry.2);
        }

        reply.ok();
    }
}

fn inner_main() -> Result<(), Error> {
    let opt = Opt::from_args();

    let creds = match (opt.username, opt.password) {
        (Some(u), Some(p)) => Some(Credentials::new(u, p)),
        _ => None,
    };

    let gh = match creds {
        Some(creds) => GitHub::with_creds(creds),
        None => GitHub::new(),
    };

    fuse::mount(GhaFs::new(gh, opt.owner, opt.repo), &opt.mount_path, &[])
        .context(InvalidMount {
            mountpoint: &opt.mount_path,
        })?;

    Ok(())
}

fn main() {
    match inner_main() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("ERROR: {}", e);
            if let Some(backtrace) = ErrorCompat::backtrace(&e) {
                println!("{}", backtrace);
            }
        }
    }
}
