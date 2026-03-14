pub const TAG_NULL: u8 = 0x00;

pub const TAG_BOOL_FALSE: u8 = 0x01;
pub const TAG_BOOL_TRUE: u8 = 0x02;

pub const TAG_UNIT: u8 = 0x03;

pub const TAG_U64: u8 = 0x10;
pub const TAG_I64: u8 = 0x11;
pub const TAG_U8: u8 = 0x14;
pub const TAG_U16: u8 = 0x15;
pub const TAG_U32: u8 = 0x16;
pub const TAG_I8: u8 = 0x17;
pub const TAG_I16: u8 = 0x18;
pub const TAG_I32: u8 = 0x19;
pub const TAG_F32: u8 = 0x12;
pub const TAG_F64: u8 = 0x13;

pub const TAG_STRING: u8 = 0x20;
pub const TAG_BYTES: u8 = 0x21;

// Physical length-prefixed encodings.
// The existing TAG_STRING/TAG_BYTES are the LEN32 form.
pub const TAG_STRING_LEN8: u8 = 0x22;
pub const TAG_STRING_LEN16: u8 = 0x23;
pub const TAG_BYTES_LEN8: u8 = 0x24;
pub const TAG_BYTES_LEN16: u8 = 0x25;

pub const TAG_ARRAY: u8 = 0x30;
pub const TAG_MAP: u8 = 0x35;

// Physical container encodings (paired widths for count + payload_len).
// The existing TAG_ARRAY/TAG_MAP are the LEN32 form.
pub const TAG_ARRAY_LEN8: u8 = 0x31;
pub const TAG_ARRAY_LEN16: u8 = 0x32;
pub const TAG_MAP_LEN8: u8 = 0x36;
pub const TAG_MAP_LEN16: u8 = 0x37;
