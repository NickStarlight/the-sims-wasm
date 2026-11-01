//! Validated version nodes with stable ANSI strings appended to the Unicode resource.
use super::super::text;

pub(super) struct Node {
    pub key: String,
    pub length: usize,
    pub value: usize,
    pub size: usize,
    pub ansi: Option<Vec<u8>>,
    pub children: Vec<Node>,
}
fn word(bytes: &[u8], offset: usize) -> Result<u16, u32> {
    let value = bytes.get(offset..offset + 2).ok_or(13u32)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn align(value: usize) -> usize {
    (value + 3) & !3
}
fn parse(bytes: &[u8], start: usize, end: usize, depth: u8) -> Result<Node, u32> {
    if depth > 8 || start + 6 > end {
        return Err(13);
    }
    let length = word(bytes, start)? as usize;
    let count = word(bytes, start + 2)? as usize;
    let kind = word(bytes, start + 4)?;
    if kind > 1 || length < 6 || start + length > end {
        return Err(13);
    }
    let end = start + length;
    let mut cursor = start + 6;
    let mut key = Vec::new();
    loop {
        if cursor + 2 > end {
            return Err(13);
        }
        let unit = word(bytes, cursor)?;
        cursor += 2;
        if unit == 0 {
            break;
        }
        key.push(unit);
    }
    let value = align(cursor);
    let size = count * if kind == 1 { 2 } else { 1 };
    if size != 0 && value + size > end {
        return Err(13);
    }
    let ansi = if kind == 1 && size != 0 {
        let mut units = Vec::new();
        for offset in (value..value + size).step_by(2) {
            let unit = word(bytes, offset)?;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        let mut output: Vec<u8> = char::decode_utf16(units)
            .map(|character| {
                character
                    .ok()
                    .and_then(|character| u16::try_from(u32::from(character)).ok())
                    .and_then(text::encode_unit)
                    .unwrap_or(b'?')
            })
            .collect();
        output.push(0);
        Some(output)
    } else {
        None
    };
    let mut children = Vec::new();
    cursor = align(value + size);
    while cursor + 6 <= end {
        let child = parse(bytes, cursor, end, depth + 1)?;
        cursor = align(cursor + child.length);
        children.push(child);
    }
    Ok(Node {
        key: String::from_utf16(&key).map_err(|_| 13u32)?,
        length,
        value,
        size,
        ansi,
        children,
    })
}
fn append(node: &mut Node, packed: &mut Vec<u8>) {
    if let Some(ansi) = &node.ansi {
        node.value = packed.len();
        node.size = ansi.len();
        packed.extend(ansi);
    }
    for child in &mut node.children {
        append(child, packed);
    }
}
pub(super) fn unpack(raw: &[u8]) -> Result<(Node, Vec<u8>), u32> {
    let mut root = parse(raw, 0, raw.len(), 0)?;
    if root.key != "VS_VERSION_INFO"
        || root.size != 52
        || root.ansi.is_some()
        || raw.get(root.value..root.value + 4) != Some(&[0xbd, 0x04, 0xef, 0xfe])
    {
        return Err(13);
    }
    let mut packed = raw[..root.length].to_vec();
    packed.resize(align(root.length), 0);
    append(&mut root, &mut packed);
    Ok((root, packed))
}
