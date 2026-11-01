//! PE resource lookup through object, preferring neutral then US English.
use object::{endian::LittleEndian as LE, read::pe::PeFile32};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Name {
    Id(u16),
    Text(String),
    Version,
}

pub(super) fn resource<'a>(bytes: &'a [u8], kind_id: u16, wanted: &Name) -> Result<&'a [u8], u32> {
    let file = PeFile32::parse(bytes).map_err(|_| 13u32)?;
    let section = file.section_table();
    let directory = file
        .data_directories()
        .resource_directory(bytes, &section)
        .map_err(|_| 13u32)?
        .ok_or(1812u32)?;
    let root = directory.root().map_err(|_| 13u32)?;
    let kind = root
        .entries
        .iter()
        .find(|entry| entry.name_or_id().id() == Some(kind_id))
        .ok_or(1813u32)?;
    let names = kind
        .data(directory)
        .map_err(|_| 13u32)?
        .table()
        .ok_or(13u32)?;
    let mut selected = None;
    for entry in names.entries {
        let matches = match wanted {
            Name::Id(id) => entry.name_or_id().id() == Some(*id),
            Name::Text(text) => match entry.name_or_id().name() {
                Some(name) => name.to_string_lossy(directory).map_err(|_| 13u32)? == *text,
                None => false,
            },
            Name::Version => entry.name_or_id().id() == Some(1),
        };
        if matches {
            selected = Some(entry);
            break;
        }
    }
    let name = selected
        .or_else(|| {
            if matches!(wanted, Name::Version) {
                names.entries.first()
            } else {
                None
            }
        })
        .ok_or(1814u32)?;
    let languages = name
        .data(directory)
        .map_err(|_| 13u32)?
        .table()
        .ok_or(13u32)?;
    let language = languages
        .entries
        .iter()
        .find(|entry| entry.name_or_id().id() == Some(0))
        .or_else(|| {
            languages
                .entries
                .iter()
                .find(|entry| entry.name_or_id().id() == Some(0x409))
        })
        .or_else(|| languages.entries.first())
        .ok_or(1813u32)?;
    let data = language
        .data(directory)
        .map_err(|_| 13u32)?
        .data()
        .ok_or(13u32)?;
    let raw = section
        .pe_data_at(bytes, data.offset_to_data.get(LE))
        .ok_or(13u32)?;
    raw.get(..data.size.get(LE) as usize).ok_or(13)
}
