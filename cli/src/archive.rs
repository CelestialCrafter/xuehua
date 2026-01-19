use std::{
    io::{Write, stdin, stdout},
    os::fd::AsRawFd,
    path::Path,
};

use bytes::{Bytes, BytesMut};
use log::warn;
use tempfile::tempfile;
use xh_archive::{decoding::Decoder, encoding::Encoder, packing::Packer, unpacking::Unpacker};
use xh_reports::{compat::StdCompat, prelude::*};

use crate::options::cli::ArchiveAction;

#[derive(Debug, IntoReport)]
pub enum ArchiveActionError {
    #[message("could not execute pack action")]
    Pack,
    #[message("could not execute unpack action")]
    Unpack,
    #[message("could not execute decode action")]
    Decode,
    #[message("could not execute hash action")]
    Hash,
}

pub fn handle(action: &ArchiveAction) -> Result<(), ArchiveActionError> {
    match action {
        ArchiveAction::Pack { path } => pack(path).wrap_with(ArchiveActionError::Pack),
        ArchiveAction::Unpack { path } => unpack(path).wrap_with(ArchiveActionError::Unpack),
        ArchiveAction::Decode => decode().wrap_with(ArchiveActionError::Decode),
        ArchiveAction::Hash => hash().wrap_with(ArchiveActionError::Hash),
    }
}

fn mmapped_stdin() -> StdResult<Bytes, std::io::Error> {
    let try_map = |fd| unsafe { memmap2::Mmap::map(fd).map(Bytes::from_owner) };

    match try_map(stdin().as_raw_fd()) {
        Ok(mmap) => Ok(mmap),
        Err(err) => {
            warn!(
                error:err = err,
                suggestion = "redirect a file into stdin";
                "could not mmap stdin. attempting to copy stdin to temporary file instead"
            );

            let mut file = tempfile()?;
            std::io::copy(&mut stdin().lock(), &mut file)?;
            Ok(try_map(file.as_raw_fd())?)
        }
    }
}

fn hash() -> Result<(), ()> {
    let mut decoder = Decoder::new();
    let mut mmap = mmapped_stdin().compat().erased()?;

    decoder
        .decode_iter(&mut mmap)
        .try_for_each(|result| result.map(|_| ()))
        .erased()?;
    println!("{}", decoder.digest());

    Ok(())
}

fn decode() -> Result<(), ()> {
    let mut stdout = stdout().lock();
    for event in Decoder::new().decode_iter(&mut mmapped_stdin().compat().erased()?) {
        writeln!(stdout, "{:#?}", event.erased()?)
            .compat()
            .erased()?;
    }

    Ok(())
}

fn unpack(path: &Path) -> Result<(), ()> {
    let mut unpacker = Unpacker::new(path);
    for event in Decoder::new().decode_iter(&mut mmapped_stdin().compat().erased()?) {
        unpacker.unpack(event.erased()?).erased()?;
    }

    Ok(())
}

fn pack(path: &Path) -> Result<(), ()> {
    let mut encoder = Encoder::new();
    let mut buffer = BytesMut::with_capacity(8192);
    let mut stdout = stdout().lock();

    for event in Packer::new(path.to_path_buf()).pack_iter() {
        buffer.clear();
        encoder.encode(&mut buffer, event.erased()?);
        stdout.write_all(&buffer).compat().erased()?
    }

    Ok(())
}
