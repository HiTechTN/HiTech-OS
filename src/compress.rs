
pub mod lz4 {
    
    pub const LZ4_MAGIC: u32 = 0x184d2204;
    pub const MAX_BLOCK_SIZE: usize = 65536;

    pub fn decompress_block(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
        let mut dst_pos = 0;
        let mut src_pos = 0;

        while src_pos < src.len() {
            let token = src[src_pos];
            src_pos += 1;

            let literal_length = (token >> 4) as usize;
            let match_length = (token & 0x0f) as usize + 4;

            if literal_length > 0 {
                if dst_pos + literal_length > dst.len() {
                    return Err(());
                }
                dst[dst_pos..dst_pos + literal_length].copy_from_slice(&src[src_pos..src_pos + literal_length]);
                dst_pos += literal_length;
                src_pos += literal_length;
            }

            if src_pos >= src.len() {
                break;
            }

            let offset = (src[src_pos] as usize) | ((src[src_pos + 1] as usize) << 8);
            src_pos += 2;

            if offset == 0 || offset > dst_pos {
                return Err(());
            }

            let match_start = dst_pos - offset;
            for i in 0..match_length.min(dst.len() - dst_pos) {
                dst[dst_pos + i] = dst[match_start + i];
            }
            dst_pos += match_length.min(dst.len() - dst_pos);
        }

        Ok(dst_pos)
    }

    pub struct Lz4FrameHeader {
        pub block_dependency: bool,
        pub block_checksums: bool,
        pub content_size: bool,
        pub content_checksum: bool,
        pub dict_id: bool,
        pub block_max_size: u32,
        pub content_size_value: u64,
    }

    impl Lz4FrameHeader {
        pub fn parse(src: &[u8]) -> Result<Lz4FrameHeader, ()> {
            if src.len() < 7 || u32::from_le_bytes(src[0..4].try_into().unwrap()) != LZ4_MAGIC {
                return Err(());
            }

            let flags = src[4];
            let bd = src[5];

            Ok(Lz4FrameHeader {
                block_dependency: (flags & 0x08) != 0,
                block_checksums: (flags & 0x10) != 0,
                content_size: (flags & 0x08) != 0,
                content_checksum: (flags & 0x10) != 0,
                dict_id: (flags & 0x01) != 0,
                block_max_size: match (bd >> 4) & 0x07 {
                    4 => 65536,
                    5 => 262144,
                    6 => 1048576,
                    7 => 4194304,
                    _ => 65536,
                },
                content_size_value: 0,
            })
        }
    }

    pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
        let _header = Lz4FrameHeader::parse(src)?;
        let mut src_pos = 7;

        if _header.content_size {
            src_pos += 8;
        }
        if _header.dict_id {
            src_pos += 4;
        }

        let mut dst_pos = 0;
        while src_pos + 4 <= src.len() {
            let block_size = u32::from_le_bytes(src[src_pos..src_pos + 4].try_into().unwrap()) as i32;
            src_pos += 4;

            if block_size == 0 {
                break;
            }

            let uncompressed = if block_size > 0 {
                let size = block_size as usize;
                decompress_block(&src[src_pos..src_pos + size], &mut dst[dst_pos..])?
            } else {
                let size = (-block_size) as usize;
                dst[dst_pos..dst_pos + size].copy_from_slice(&src[src_pos..src_pos + size]);
                size
            };

            dst_pos += uncompressed;
            src_pos += if block_size > 0 { block_size as usize } else { (-block_size) as usize };

            if _header.block_checksums {
                src_pos += 4;
            }
        }

        Ok(dst_pos)
    }
}

pub mod zlib {
    pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
        if src.len() < 2 {
            return Err(());
        }
        let cmf = src[0];
        let flg = src[1];
        if (cmf as u16 * 256 + flg as u16) % 31 != 0 {
            return Err(());
        }
        let mut src_pos = 2;
        let mut dst_pos = 0;
        while src_pos < src.len() - 4 {
            let bfinal = (src[src_pos] & 0x01) != 0;
            let btype = (src[src_pos] >> 1) & 0x03;
            src_pos += 1;
            match btype {
                1 => {
                    let mut bit_pos = 0;
                    loop {
                        let mut lit = 0u16;
                        for i in 0..3 {
                            if src_pos + i < src.len() {
                                lit |= (src[src_pos + i] as u16) << (8 * i);
                            }
                        }
                        src_pos += 3;
                        if dst_pos + (lit as usize) + 3 > dst.len() || src_pos + (lit as usize) > src.len() {
                            return Err(());
                        }
                        dst[dst_pos..dst_pos + (lit as usize) + 3].copy_from_slice(&src[src_pos..src_pos + (lit as usize)]);
                        dst_pos += (lit as usize) + 3;
                        src_pos += lit as usize;
                        if bit_pos >= 7 {
                            break;
                        }
                        bit_pos += 1;
                    }
                }
                0 => {
                    while src_pos < src.len() - 1 {
                        let len = src[src_pos] as usize;
                        if len == 0 {
                            src_pos += 1;
                            break;
                        }
                        if dst_pos + len > dst.len() || src_pos + 1 + len > src.len() {
                            return Err(());
                        }
                        dst[dst_pos..dst_pos + len].copy_from_slice(&src[src_pos + 1..src_pos + 1 + len]);
                        dst_pos += len;
                        src_pos += 1 + len;
                    }
                }
                _ => return Err(()),
            }
            if bfinal {
                break;
            }
        }
        Ok(dst_pos)
    }

    pub fn compress(src: &[u8]) -> alloc::vec::Vec<u8> {
        let mut result = alloc::vec::Vec::with_capacity(src.len() + 12);
        result.push(0x78);
        result.push(0x01);
        let mut i = 0;
        while i < src.len() {
            let chunk_end = (i + 65535).min(src.len());
            let chunk = &src[i..chunk_end];
            let chunk_len = chunk.len();
            result.push(if chunk_end >= src.len() { 0x01 } else { 0x00 });
            result.push((chunk_len & 0xff) as u8);
            result.push(((chunk_len >> 8) & 0xff) as u8);
            result.push((!chunk_len & 0xff) as u8);
            result.push(((!chunk_len >> 8) & 0xff) as u8);
            result.extend_from_slice(chunk);
            i = chunk_end;
        }
        result.push(0x01);
        result.push(0x00);
        result.push(0x00);
        result.push(0xff);
        result.push(0xff);
        result
    }
}

pub mod huffman {
    
    pub struct HuffmanTree {
        pub symbols: [u32; 286],
        pub length: u32,
    }

    impl HuffmanTree {
        pub fn new() -> Self {
            HuffmanTree {
                symbols: [0; 286],
                length: 0,
            }
        }

        pub fn decode(&self, data: &[u8], bit_pos: &mut usize) -> Option<u32> {
            if *bit_pos >= data.len() * 8 {
                return None;
            }
            let byte = *bit_pos / 8;
            let bit = *bit_pos % 8;
            let index = ((data[byte] >> bit) & 0x01) as usize;
            *bit_pos += 1;
            Some(self.symbols[index])
        }
    }
}

pub mod runlength {
    use alloc::vec::Vec;
    pub fn encode(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let byte = data[i];
            let mut count = 1;
            while i + count < data.len() && count < 255 && data[i + count] == byte {
                count += 1;
            }
            if count >= 3 || byte == 0 {
                result.push(0);
                result.push(byte);
                result.push(count as u8);
            } else {
                for _ in 0..count {
                    result.push(byte);
                }
            }
            i += count;
        }
        result
    }

    pub fn decode(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < data.len() {
            if data[i] == 0 && i + 2 < data.len() {
                let byte = data[i + 1];
                let count = data[i + 2] as usize;
                for _ in 0..count {
                    result.push(byte);
                }
                i += 3;
            } else {
                result.push(data[i]);
                i += 1;
            }
        }
        result
    }
}

pub mod huffman_deflate {
    

    pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
        let mut src_pos = 0;
        let mut dst_pos = 0;
        while src_pos < src.len() && dst_pos < dst.len() {
            let _bfinal = src[src_pos] & 0x01;
            let btype = (src[src_pos] >> 1) & 0x03;
            src_pos += 1;
            match btype {
                0 => {
                    if src_pos + 4 > src.len() {
                        return Err(());
                    }
                    let len = u16::from_le_bytes(src[src_pos..src_pos + 2].try_into().unwrap()) as usize;
                    let _nlen = u16::from_le_bytes(src[src_pos + 2..src_pos + 4].try_into().unwrap()) as usize;
                    src_pos += 4;
                    if src_pos + len > src.len() || dst_pos + len > dst.len() {
                        return Err(());
                    }
                    dst[dst_pos..dst_pos + len].copy_from_slice(&src[src_pos..src_pos + len]);
                    dst_pos += len;
                    src_pos += len;
                }
                2 => {
                    if src_pos + 2 > src.len() {
                        return Err(());
                    }
                    let hlit = src[src_pos] as usize + ((src[src_pos + 1] as usize & 0x1f) << 8) as usize;
                    let hdist = ((src[src_pos + 1] >> 5) & 0x1f) as usize;
                    let hclen = ((src[src_pos + 2] >> 2) & 0x0f) as usize;
                    if hlit + hdist + hclen > 286 + 30 + 19 {
                        return Err(());
                    }
                    let _code_lengths_count = 4 * (hclen + 4);

                    let num_literals = hlit + 257;
                    let num_distances = hdist + 1;
                    let total = num_literals + num_distances;

                    if total > 320 {
                        return Err(());
                    }

                    return Err(());
                }
                1 | 3 => return Err(()),
                _ => return Err(()),
            }
        }
        Ok(dst_pos)
    }

    pub struct FixedHuffman;
    impl FixedHuffman {
        pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, ()> {
            let mut src_pos = 0;
            let mut dst_pos = 0;
            while src_pos < src.len() && dst_pos < dst.len() {
                let byte = src[src_pos];
                dst[dst_pos] = byte;
                dst_pos += 1;
                src_pos += 1;
            }
            Ok(dst_pos)
        }
    }
}