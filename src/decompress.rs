use byteorder::{ByteOrder, LittleEndian};

struct Decoder<'a> {
    input: &'a [u8],
    output: &'a mut Vec<u8>,
    token: u8,
}

impl<'a> Decoder<'a> {
    fn take_imp(input: &mut &'a [u8], n: usize) -> anyhow::Result<&'a [u8]> {
        if input.len() < n {
            Err(anyhow::Error::msg("Expected another byte, found none."))
        } else {
            let res = Ok(&input[..n]);
            *input = &input[n..];
            res
        }
    }

    fn take(&mut self, n: usize) -> anyhow::Result<&[u8]> {
        Self::take_imp(&mut self.input, n)
    }

    fn output(output: &mut Vec<u8>, buf: &[u8]) {
        output.extend_from_slice(&buf[..buf.len()]);
    }

    fn duplicate(&mut self, start: usize, match_length: usize) {
        for i in start..start + match_length {
            let b = self.output[i];
            self.output.push(b);
        }
    }

    fn read_integer(&mut self) -> anyhow::Result<usize> {
        let mut n = 0;
        while {
            let extra = self.take(1)?[0];
            n += extra as usize;
            extra == 0xFF
        } {}

        Ok(n)
    }

    fn read_u16(&mut self) -> anyhow::Result<u16> {
        Ok(LittleEndian::read_u16(self.take(2)?))
    }

    fn read_literal_section(&mut self) -> anyhow::Result<()> {
        let mut literal = (self.token >> 4) as usize;
        if literal == 15 {
            literal += self.read_integer()?;
        }
        Self::output(&mut self.output, Self::take_imp(&mut self.input, literal)?);

        Ok(())
    }

    fn read_duplicate_section(&mut self) -> anyhow::Result<()> {
        let offset = self.read_u16()?;
        let mut match_length = (4 + (self.token & 0xF)) as usize;
        if match_length == 4 + 15 {
            match_length += self.read_integer()?;
        }
        let start = self.output.len().wrapping_sub(offset as usize);
        if start < self.output.len() {
            self.duplicate(start, match_length);

            Ok(())
        } else {
            Err(anyhow::Error::msg(
                "The offset to copy is not contained in the decompressed buffer.",
            ))
        }
    }

    fn complete(&mut self) -> anyhow::Result<()> {
        while !self.input.is_empty() {
            self.token = self.take(1)?[0];
            self.read_literal_section()?;
            if self.input.is_empty() {
                break;
            }
            self.read_duplicate_section()?;
        }

        Ok(())
    }
}

pub fn decompress_into(input: &[u8], output: &mut Vec<u8>) -> anyhow::Result<()> {
    Decoder {
        input: input,
        output: output,
        token: 0,
    }
    .complete()?;

    Ok(())
}

pub fn decompress(input: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut vec = Vec::with_capacity(4096);
    decompress_into(input, &mut vec)?;
    Ok(vec)
}
