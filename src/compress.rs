use byteorder::{ByteOrder, NativeEndian};

const DICTIONARY_SIZE: usize = 4096;

#[derive(Debug)]
struct Block {
    lit_len: usize,
    dup: Option<Duplicate>,
}

#[derive(Copy, Clone, Debug)]
struct Duplicate {
    offset: u16,
    extra_bytes: usize,
}

struct Encoder<'a> {
    input: &'a [u8],
    output: &'a mut Vec<u8>,
    cur: usize,
    dict: [usize; DICTIONARY_SIZE],
}

impl<'a> Encoder<'a> {
    fn go_forward(&mut self, steps: usize) -> bool {
        for _ in 0..steps {
            self.insert_cursor();
            self.cur += 1;
        }
        self.cur <= self.input.len()
    }

    fn insert_cursor(&mut self) {
        if self.remaining_batch() {
            self.dict[self.get_cur_hash()] = self.cur;
        }
    }

    fn remaining_batch(&self) -> bool {
        self.cur + 4 < self.input.len()
    }

    fn get_cur_hash(&self) -> usize {
        let mut x = self.get_batch_at_cursor().wrapping_mul(0xa4d94a4f);
        let a = x >> 16;
        let b = x >> 30;
        x ^= a >> b;
        x = x.wrapping_mul(0xa4d94a4f);

        x as usize % DICTIONARY_SIZE
    }

    fn get_batch(&self, n: usize) -> u32 {
        debug_assert!(self.remaining_batch(), "Reading a partial batch.");

        NativeEndian::read_u32(&self.input[n..])
    }

    fn get_batch_at_cursor(&self) -> u32 {
        self.get_batch(self.cur)
    }

    fn find_duplicate(&self) -> Option<Duplicate> {
        if !self.remaining_batch() {
            return None;
        }

        let candidate = self.dict[self.get_cur_hash()];

        if candidate != !0 && self.get_batch(candidate) == self.get_batch_at_cursor() && self.cur - candidate <= 0xFFFF
        {
            let ext = self.input[self.cur + 4..]
                .iter()
                .zip(&self.input[candidate + 4..])
                .take_while(|&(a, b)| a == b)
                .count();

            Some(Duplicate {
                offset: (self.cur - candidate) as u16,
                extra_bytes: ext,
            })
        } else {
            None
        }
    }

    fn write_integer(&mut self, mut n: usize) {
        while n >= 0xFF {
            n -= 0xFF;
            self.output.push(0xFF);
        }
        self.output.push(n as u8);
    }

    fn pop_block(&mut self) -> Block {
        let mut lit = 0;

        loop {
            if let Some(dup) = self.find_duplicate() {
                self.go_forward(dup.extra_bytes + 4);
                return Block {
                    lit_len: lit,
                    dup: Some(dup),
                };
            }

            if !self.go_forward(1) {
                return Block {
                    lit_len: lit,
                    dup: None,
                };
            }
            lit += 1;
        }
    }

    fn complete(&mut self) {
        loop {
            let start = self.cur;
            let block = self.pop_block();
            let mut token = if block.lit_len < 0xF {
                (block.lit_len as u8) << 4
            } else {
                0xF0
            };

            let dup_extra_len = block.dup.map_or(0, |x| x.extra_bytes);
            token |= if dup_extra_len < 0xF { dup_extra_len as u8 } else { 0xF };
            self.output.push(token);
            if block.lit_len >= 0xF {
                self.write_integer(block.lit_len - 0xF);
            }

            self.output.extend_from_slice(&self.input[start..start + block.lit_len]);

            if let Some(Duplicate { offset, .. }) = block.dup {
                self.output.push(offset as u8);
                self.output.push((offset >> 8) as u8);

                if dup_extra_len >= 0xF {
                    self.write_integer(dup_extra_len - 0xF);
                }
            } else {
                break;
            }
        }
    }
}

pub fn compress_into(input: &[u8], output: &mut Vec<u8>) {
    Encoder {
        input: input,
        output: output,
        cur: 0,
        dict: [!0; DICTIONARY_SIZE],
    }
    .complete();
}

pub fn compress(input: &[u8]) -> Vec<u8> {
    let mut vec = Vec::with_capacity(input.len());
    compress_into(input, &mut vec);
    vec
}
