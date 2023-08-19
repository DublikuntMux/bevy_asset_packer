use aes::{Aes128, cipher::BlockEncrypt};
use belt_ctr::cipher::generic_array::GenericArray;

fn increment_counter(counter: &mut [u8]) {
    for byte in counter.iter_mut().rev() {
        *byte = byte.wrapping_add(1);
        if *byte != 0 {
            break;
        }
    }
}

pub fn encrypt_ctr(cipher: &Aes128, plaintext: &[u8]) -> Vec<u8> {
    let mut ciphertext = Vec::with_capacity(plaintext.len());

    let iv = [57, 206, 202, 7, 215, 17, 43, 219, 131, 171, 7, 214, 85, 12, 129, 176];

    let mut counter = iv.to_vec();
    counter.resize(16, 0);

    let mut block = GenericArray::clone_from_slice(&counter);

    for chunk in plaintext.chunks(16) {
        cipher.encrypt_block(&mut block);

        let encrypted_chunk: Vec<u8> = block.iter().zip(chunk.iter()).map(|(b1, b2)| b1 ^ b2).collect();

        ciphertext.extend_from_slice(&encrypted_chunk);
        increment_counter(&mut counter);
        block = GenericArray::clone_from_slice(&counter);
    }

    ciphertext
}

pub fn decrypt_ctr(cipher: &Aes128, ciphertext: &[u8]) -> Vec<u8> {
    encrypt_ctr(cipher, ciphertext)
}
