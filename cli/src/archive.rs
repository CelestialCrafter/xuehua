use std::{
    io::{Error as IOError, Write, stdin, stdout},
    os::fd::AsRawFd,
};

use bytes::{Bytes, BytesMut};
use log::warn;
use tempfile::tempfile;
use xh_archive::{
    Event, decoding::Decoder, encoding::Encoder, hashing::Hasher, packing::Packer,
    unpacking::Unpacker,
};

use crate::options::cli::ArchiveAction;

fn mmapped_stdin() -> Result<Bytes, IOError> {
    let try_map = |fd| unsafe { memmap2::Mmap::map(fd).map(Bytes::from_owner) };

    match try_map(stdin().as_raw_fd()) {
        Ok(mmap) => Ok(mmap),
        Err(err) => {
            warn!("could not mmap stdin: {err}");
            warn!("attempting to copy stdin to temporary file instead");

            let mut file = tempfile()?;
            std::io::copy(&mut stdin().lock(), &mut file)?;
            try_map(file.as_raw_fd())
        }
    }
}

pub fn handle(action: &ArchiveAction) -> Result<(), eyre::Error> {
    match action {
        ArchiveAction::Pack { path } => {
            let mut encoder = Encoder::new();
            let mut buffer = BytesMut::with_capacity(8192);
            let mut stdout = stdout().lock();

            for event in Packer::new(path.clone()).pack_iter() {
                buffer.clear();
                encoder.encode(&mut buffer, event?);
                stdout.write_all(&buffer)?;
            }
        }
        ArchiveAction::Unpack { path } => {
            let mut unpacker = Unpacker::new(path);
            for event in Decoder::new().decode_iter(&mut mmapped_stdin()?) {
                unpacker.unpack(event?)?;
            }
        }
        ArchiveAction::Decode => {
            let mut stdout = stdout().lock();
            for event in Decoder::new().decode_iter(&mut mmapped_stdin()?) {
                writeln!(stdout, "{:#?}", event?)?;
            }
        }
        ArchiveAction::Hash { each_object: each_event } => {
            let mut decoder = Decoder::new();
            let mut mmap = mmapped_stdin()?;

            let hashes = decoder
                .decode_iter(&mut mmap)
                .filter_map(|result| match result {
                    Ok(event) => match event {
                        Event::Object(object) => Some(Ok(Hasher::hash(object))),
                        _ => None,
                    },
                    Err(err) => Some(Err(err)),
                });

            if *each_event {
                let mut stdout = stdout().lock();
                for hash in hashes {
                    writeln!(stdout, "{}", hash?)?;
                }
            } else {
                let hash = Hasher::aggregate(hashes.collect::<Result<Vec<_>, _>>()?);
                println!("{hash}");
            }
        }
    }

    Ok(())
}
