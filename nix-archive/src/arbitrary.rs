use core::ops::ControlFlow;
use alloc::vec::Vec;

use arbitrary::Arbitrary;
use bytes::Bytes;

use crate::Event;

#[derive(Debug)]
pub struct ArbitraryObject(pub Vec<Event>);

impl Arbitrary<'_> for ArbitraryObject {
    fn arbitrary(u: &mut arbitrary::Unstructured) -> arbitrary::Result<Self> {
        let make_bytes =
            |u: &mut arbitrary::Unstructured| u.arbitrary().map(bytes::Bytes::copy_from_slice);
        let mut events = Vec::new();

        match u.choose_index(3)? {
            0 => {
                let mut size = u.arbitrary_len::<&[u8]>()?;
                events.push(Event::Regular {
                    executable: u.arbitrary()?,
                    size: size as u64,
                });

                if size == 0 {
                    events.push(Event::RegularContentChunk(Bytes::new()));
                }

                while size != 0 {
                    let chunk_size = u.int_in_range(1..=size)?;
                    size -= chunk_size;

                    let data = u.bytes(chunk_size)?;
                    events.push(Event::RegularContentChunk(Bytes::copy_from_slice(data)));
                }
            }
            1 => events.push(Event::Symlink {
                target: make_bytes(u)?,
            }),
            2 => {
                events.push(Event::Directory);

                const MAX_FILES: u32 = 8;
                u.arbitrary_loop(None, Some(MAX_FILES), |u| {
                    events.push(Event::DirectoryEntry {
                        name: make_bytes(u)?,
                    });
                    events.extend(Self::arbitrary(u)?.0);

                    Ok(ControlFlow::Continue(()))
                })?;

                events.push(Event::DirectoryEnd);
            }
            _ => unreachable!(),
        }

        Ok(Self(events))
    }
}

#[derive(Debug)]
pub struct ArbitraryNar(pub Vec<Event>);

impl Arbitrary<'_> for ArbitraryNar {
    #[inline]
    fn arbitrary(u: &mut arbitrary::Unstructured) -> Result<Self, arbitrary::Error> {
        let mut events = Vec::with_capacity(1);
        events.push(Event::Header);
        events.extend(ArbitraryObject::arbitrary(u)?.0);
        Ok(Self(events))
    }
}
