mod github;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory,
    ReplyEntry, Request,
};
use libc::ENOENT;
use snafu::{ErrorCompat, ResultExt, Snafu};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::iter::once;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, UNIX_EPOCH};
use structopt::StructOpt;

use crate::github::{Credentials, GitHub, Release};

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

const ROOT_DIR_ATTR: FileAttr = FileAttr {
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
    uid: 1000,
    gid: 1000,
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
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: ino as u32,
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
        uid: 1000,
        gid: 1000,
        rdev: ino as u32,
        flags: 0,
    }
}

type AssetMappings = HashMap<String, u64>;

#[derive(Debug)]
struct ReleaseMapping {
    ino: u64,
    asset_mappings: AssetMappings,
}

impl ReleaseMapping {
    fn new(ino: u64, asset_mappings: AssetMappings) -> ReleaseMapping {
        ReleaseMapping {
            ino,
            asset_mappings,
        }
    }
}

type ReleaseMappings = HashMap<String, ReleaseMapping>;

fn generate_release_mappings(releases: &[Release]) -> ReleaseMappings {
    let mapping = releases
        .iter()
        .map(|release| {
            let asset_mappings = release
                .assets
                .iter()
                .map(|asset| (asset.name.clone(), asset.id as u64 + 1))
                .collect();

            let release_mapping =
                ReleaseMapping::new(release.id as u64 + 1, asset_mappings);

            (release.tag_name.to_owned(), release_mapping)
        })
        .collect();

    mapping
}

fn find_release_mapping(
    release_mappings: &ReleaseMappings,
    ino: u64,
) -> Option<&ReleaseMapping> {
    release_mappings
        .values()
        .find(|release_mapping| release_mapping.ino == ino)
}

#[derive(Debug)]
struct GhaFs {
    releases: Arc<RwLock<Vec<Release>>>,
    release_mappings: ReleaseMappings,
}

impl GhaFs {
    fn new(mut state: GitHub, owner: String, repo: String) -> GhaFs {
        let releases = state
            .releases(&owner, &repo)
            .expect("lookup.releases GET error");

        let release_mappings = generate_release_mappings(&releases);

        GhaFs {
            releases: Arc::new(RwLock::new(releases)),
            release_mappings,
        }
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

        if parent == 1 {
            let name = name.to_str().expect("lookup.name.to_str error");
            let release_mapping = self.release_mappings.get(name);

            match release_mapping {
                Some(ReleaseMapping {
                    ino,
                    asset_mappings: _,
                }) => {
                    println!("> root dir lookup for name {}", name);
                    reply.entry(&TTL, &create_dir_attr(*ino), 0);
                }
                _ => {
                    println!("> ERROR root dir lookup for name {}", name);
                    reply.error(ENOENT);
                }
            }
        } else {
            let release_mapping =
                find_release_mapping(&self.release_mappings, parent);

            match release_mapping {
                Some(ReleaseMapping {
                    ino,
                    asset_mappings: _,
                }) => {
                    println!("> subdir lookup in parent found!");
                    reply.entry(&TTL, &create_file_attr(*ino), 0);
                }
                _ => {
                    println!("> ERROR subdir lookup in parent NOT found!");
                    reply.error(ENOENT);
                }
            }
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
        println!("read, ino: {}, offset: {}", ino, offset);

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
            1 => reply.attr(&TTL, &ROOT_DIR_ATTR),
            ino @ _ => {
                let release_mapping =
                    find_release_mapping(&self.release_mappings, ino);

                if let Some(_) = release_mapping {
                    reply.attr(&TTL, &create_dir_attr(ino))
                } else {
                    reply.attr(&TTL, &create_file_attr(ino))
                }
            }
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
        // println!("readdir, ino: {}, offset: {}", ino, offset);

        // Root has ino 1
        let entries = if ino == 1 {
            if offset == 0 {
                once((1, FileType::Directory, ".".to_owned()))
                    .chain(once((1, FileType::Directory, "..".to_owned())))
                    .chain(self.release_mappings.iter().map(
                        |(name, release_mapping)| {
                            (
                                release_mapping.ino,
                                FileType::Directory,
                                name.clone(),
                            )
                        },
                    ))
                    .collect()
            } else {
                vec![]
            }
        } else {
            if offset == 0 {
                let release_mapping =
                    find_release_mapping(&self.release_mappings, ino);

                if let Some(release_mapping) = release_mapping {
                    once((ino, FileType::Directory, ".".to_owned()))
                        .chain(once((1, FileType::Directory, "..".to_owned())))
                        .chain(release_mapping.asset_mappings.iter().map(
                            |(asset_name, &asset_id_offset)| {
                                (
                                    asset_id_offset,
                                    FileType::RegularFile,
                                    asset_name.clone(),
                                )
                            },
                        ))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        // for entry in entries.into_iter().skip(offset as usize)
        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize)
        {
            // https://github.com/libfuse/libfuse/blob/master/include/fuse.h#L72
            // For directory that does seeking (i.e. ls command)
            // offset cannot be zero
            let (ino, kind, name) = entry;
            reply.add(ino, (i + 1) as i64, kind, name);
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
