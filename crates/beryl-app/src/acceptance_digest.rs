//! Dependency-free streaming SHA-256 for bounded acceptance artifacts.

/// Incrementally computes a SHA-256 digest.
///
/// The hasher accepts arbitrary chunk boundaries. Finalization consumes it so a
/// completed digest cannot accidentally be extended.
#[derive(Clone, Debug)]
pub(crate) struct Sha256 {
    state: [u32; 8],
    block: [u8; BLOCK_BYTES],
    block_len: usize,
    message_len_bytes: u64,
}

const BLOCK_BYTES: usize = 64;
const DIGEST_BYTES: usize = 32;
const INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];
const ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            block: [0; BLOCK_BYTES],
            block_len: 0,
            message_len_bytes: 0,
        }
    }

    pub(crate) fn digest(bytes: &[u8]) -> [u8; DIGEST_BYTES] {
        let mut hasher = Self::new();
        hasher.update(bytes);
        hasher.finalize()
    }

    pub(crate) fn digest_hex(bytes: &[u8]) -> String {
        hex_encode_upper(&Self::digest(bytes))
    }

    pub(crate) fn update(&mut self, mut bytes: &[u8]) {
        self.message_len_bytes = self
            .message_len_bytes
            .checked_add(u64::try_from(bytes.len()).expect("input length exceeds SHA-256 limit"))
            .expect("SHA-256 input exceeds 2^64 - 1 bytes");

        if self.block_len != 0 {
            let to_copy = (BLOCK_BYTES - self.block_len).min(bytes.len());
            self.block[self.block_len..self.block_len + to_copy].copy_from_slice(&bytes[..to_copy]);
            self.block_len += to_copy;
            bytes = &bytes[to_copy..];

            if self.block_len < BLOCK_BYTES {
                return;
            }
            let block = self.block;
            self.process_block(&block);
            self.block_len = 0;
        }

        while bytes.len() >= BLOCK_BYTES {
            let (block, remaining) = bytes.split_at(BLOCK_BYTES);
            self.process_block(block);
            bytes = remaining;
        }

        self.block[..bytes.len()].copy_from_slice(bytes);
        self.block_len = bytes.len();
    }

    pub(crate) fn finalize(mut self) -> [u8; DIGEST_BYTES] {
        let bit_len = self
            .message_len_bytes
            .checked_mul(8)
            .expect("SHA-256 input exceeds 2^61 - 1 bytes");

        self.block[self.block_len] = 0x80;
        self.block_len += 1;

        if self.block_len > 56 {
            self.block[self.block_len..].fill(0);
            let block = self.block;
            self.process_block(&block);
            self.block_len = 0;
        }

        self.block[self.block_len..56].fill(0);
        self.block[56..].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.block;
        self.process_block(&block);

        let mut digest = [0; DIGEST_BYTES];
        for (word, output) in self.state.into_iter().zip(digest.chunks_exact_mut(4)) {
            output.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    pub(crate) fn finalize_hex(self) -> String {
        hex_encode_upper(&self.finalize())
    }

    fn process_block(&mut self, block: &[u8]) {
        debug_assert_eq!(block.len(), BLOCK_BYTES);

        let mut schedule = [0_u32; 64];
        for (word, bytes) in schedule[..16].iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().expect("SHA-256 block word length"));
        }
        for index in 16..schedule.len() {
            let sigma0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let sigma1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(sigma0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(sigma1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (constant, word) in ROUND_CONSTANTS.into_iter().zip(schedule) {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(constant)
                .wrapping_add(word);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_encode_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}
