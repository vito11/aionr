/*******************************************************************************
 * Copyright (c) 2018-2019 Aion foundation.
 *
 *     This file is part of the aion network project.
 *
 *     The aion network project is free software: you can redistribute it
 *     and/or modify it under the terms of the GNU General Public License
 *     as published by the Free Software Foundation, either version 3 of
 *     the License, or any later version.
 *
 *     The aion network project is distributed in the hope that it will
 *     be useful, but WITHOUT ANY WARRANTY; without even the implied
 *     warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 *     See the GNU General Public License for more details.
 *
 *     You should have received a copy of the GNU General Public License
 *     along with the aion network project source files.
 *     If not, see <https://www.gnu.org/licenses/>.
 *
 ******************************************************************************/

// calculate difficulty.
use std::collections::HashSet;
use bytes::bytes_to_i32s;
use bytes::i32_to_bytes;
use bytes::i32_to_bytes_le;
use blake2b::Blake2b;
use std::ptr;

pub struct EquihashValidator {
    n: i32,
    k: i32,
    //    indices_per_hash_output: i32,
    indices_hash_length: usize,
    //    hash_output: i32,
    collision_bit_length: i32,
    solution_width: i32,
}

impl EquihashValidator {
    pub fn new(n: i32, k: i32) -> EquihashValidator {
        let indices_per_hash_output = 512 / n;
        let indices_hash_length = (n + 7) / 8;
        //        let hash_output = indices_per_hash_output * indices_hash_length;
        let collision_bit_length = n / (k + 1);
        let solution_width = (1 << k) * (collision_bit_length + 1) / 8;
        debug!(target: "equihash", "equihash validator - solution_width={}", solution_width);
        debug!(target: "equihash", "equihash validator - collision_bit_length={}", collision_bit_length);
        //        trace!(target: "equihash", "equihash validator - hash_output={}", hash_output);
        debug!(target: "equihash", "equihash validator - indices_hash_length={}", indices_hash_length);
        debug!(target: "equihash", "equihash validator - indices_per_hash_output={}", indices_per_hash_output);
        EquihashValidator {
            n,
            k,
            //            indices_per_hash_output,
            indices_hash_length: indices_hash_length as usize,
            //            hash_output,
            collision_bit_length,
            solution_width,
        }
    }

    pub fn is_valid_solution(&self, solution: &[u8], block_header: &[u8], nonce: &[u8]) -> bool {
        if solution.len() as i32 != self.solution_width {
            error!(target: "equihash", "Invalid solution width: {}", solution.len());
            return false;
        }

        let indices: Vec<i32> = self.get_indices_from_minimal(solution, self.collision_bit_length);
        if self.has_duplicate(&indices) {
            error!(target: "equihash", "Invalid solution - duplicate solution index");
            return false;
        }

        let mut personalization: Vec<u8> = Vec::with_capacity(16);
        personalization.extend_from_slice("AION0PoW".as_bytes());
        personalization.extend_from_slice(&i32_to_bytes_le(self.n));
        personalization.extend_from_slice(&i32_to_bytes_le(self.k));
        let native_hash = self.get_solution_hash(
            &personalization.as_slice(),
            nonce,
            indices.as_slice(),
            block_header,
        );

        let mut hash: Vec<u8> = Vec::with_capacity(self.indices_hash_length);
        for _i in 0..self.indices_hash_length {
            hash.push(0u8);
        }
        self.verify(&indices, 0, hash.as_mut_slice(), self.k, &native_hash)
    }

    fn has_duplicate(&self, indices: &Vec<i32>) -> bool {
        let mut set: HashSet<i32> = HashSet::with_capacity(512);
        for index in indices {
            if !set.insert(*index) {
                return true;
            }
        }
        false
    }

    fn get_indices_from_minimal(&self, minimal: &[u8], c_bit_len: i32) -> Vec<i32> {
        let len_indices = 8 * 4 * minimal.len() / (c_bit_len as usize + 1);
        let byte_pad = 4 - ((c_bit_len + 1) + 7) / 8;

        let mut arr: Vec<u8> = Vec::with_capacity(len_indices);
        for _i in 0..len_indices {
            arr.push(0u8);
        }
        super::extend_array(minimal, arr.as_mut_slice(), c_bit_len + 1, byte_pad);

        let ret_len = arr.len() / 4;
        let mut ret: Vec<i32> = Vec::with_capacity(ret_len);
        for _i in 0..ret_len {
            ret.push(0);
        }
        bytes_to_i32s(arr.as_slice(), ret.as_mut_slice(), true);
        ret
    }

    fn get_solution_hash(
        &self,
        personalization: &[u8],
        nonce: &[u8],
        indices: &[i32],
        header: &[u8],
    ) -> Vec<[u8; 27]>
    {
        let hashesperblake: i32 = 2;

        let mut param = [0u8; 64];
        param[0] = 54;
        param[2] = 1;
        param[3] = 1;
        param[48..64].copy_from_slice(&personalization[..16]);

        let mut out: Vec<[u8; 27]> = Vec::new();
        let indice_len = indices.len();
        for i in 0..indice_len {
            let mut blake2b = Blake2b::with_params(&param);
            blake2b.update(header);
            blake2b.update(nonce);
            let leb: i32 = (indices[i] / hashesperblake).to_le();
            blake2b.update(&i32_to_bytes(leb));
            let mut blakehash = [0u8; 54];
            blake2b.finalize(&mut blakehash);

            unsafe {
                let mut index_hash: [u8; 27] = [0u8; 27];

                let s = ((indices[i] % hashesperblake) * (self.n + 7) / 8) as usize;
                ptr::copy_nonoverlapping(
                    blakehash[s..].as_ptr(),
                    index_hash.as_mut_ptr(),
                    ((self.n + 7) / 8) as usize,
                );
                out.push(index_hash);
            }
        }

        out
    }

    fn verify(
        &self,
        indices: &Vec<i32>,
        index: i32,
        hash: &mut [u8],
        round: i32,
        hashes: &Vec<[u8; 27]>,
    ) -> bool
    {
        if round == 0 {
            return true;
        }

        let index1 = index + (1 << ((round - 1) % 32));
        if indices[index as usize] >= indices[index1 as usize] {
            error!(target: "equihash", "Solution validation failed - indices out of order");
            return false;
        }

        let mut hash0 = hashes[index as usize];
        let mut hash1 = hashes[index1 as usize];
        let verify0 = self.verify(&indices, index, &mut hash0, round - 1, &hashes);
        if !verify0 {
            error!(target: "equihash", "Solution validation failed - unable to verify left subtree");
            return false;
        }

        let verify1 = self.verify(&indices, index1, &mut hash1, round - 1, &hashes);
        if !verify1 {
            error!(target: "equihash", "Solution validation failed - unable to verify right subtree");
            return false;
        }

        for i in 0..(self.indices_hash_length) {
            hash[i] = hash0[i] ^ hash1[i];
        }

        let mut bits = self.n;
        if round < self.k {
            bits = self.collision_bit_length;
        }
        for i in 0..((bits / 8) as usize) {
            if hash[i] != 0 {
                error!(target: "equihash", "Solution validation failed - Non-zero XOR");
                return false;
            }
        }

        // check remainder bits
        if (bits % 8) > 0 && (hash[(bits / 8) as usize] >> (8 - (bits % 8))) != 0 {
            error!(target: "equihash", "Solution validation failed - Non-zero XOR");
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::EquihashValidator;
    use hex::FromHex;

    #[test]
    fn test_has_duplicate_true() {
        let duplicate_array = vec![1, 2, 1];
        let validator = EquihashValidator::new(1, 1);
        let result = validator.has_duplicate(&duplicate_array);
        assert_eq!(result, true);
    }

    #[test]
    fn test_has_duplicate_false() {
        let duplicate_array = vec![1, 2, 3];
        let validator = EquihashValidator::new(1, 1);
        let result = validator.has_duplicate(&duplicate_array);
        assert_eq!(result, false);
    }

    #[test]
    fn test_get_solution_hash() {
        let personalization = [0u8; 16];
        let nonce = [0u8; 10];
        let header = [0u8; 10];
        let indices = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let validator = EquihashValidator::new(210, 9);
        let result = validator.get_solution_hash(&personalization, &nonce, &indices, &header);
        let mut expected: String = String::new();
        expected.push_str("82271c24f9aaba808c71cd8c65d9cda08648e56277579750be313f".into());
        expected.push_str("2c0288d19f4e8529a98462d51a91e9f79f8a351a50310bcca87bd8".into());
        expected.push_str("85f2b3bc6b86acc92fd223cdeb98650f62d058378c61c56c309855".into());
        expected.push_str("891e6578ae82d3a1458cc263c8a96d5132afb3dff2853c2f9da35d".into());
        expected.push_str("3040b6e9db1b1a3334065d0071bcf901bca72591a99fbc35a47923".into());
        expected.push_str("280da7323dcd16cb428b52fc96a2a91b444c9857bd7e3d2616155a".into());
        expected.push_str("a6b3dcdf27128fbc84a302278d966d486e586d49d80e960f77e878".into());
        expected.push_str("b89c0af5835d1a10978cf58e6f6b0200388214dd034f49df6ebd36".into());
        expected.push_str("c3afe5a262aa7239916637a6cd5eccd911c9c0db16e334e67cc153".into());
        expected.push_str("612bd504c6855c84670582aa1e6e1e4299b6226bfb32fa3d10f323".into());

        let mut actual: String = String::new();
        for r in result {
            for i in 0..r.len() {
                actual.push_str(&format!("{:02x}", r[i]));
            }
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_get_solution_hash2() {
        let personalization: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let nonce: [u8; 20] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        ];
        let header: [u8; 32] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0a, 0x0b,
        ];
        let indices = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let validator = EquihashValidator::new(210, 9);
        let result = validator.get_solution_hash(&personalization, &nonce, &indices, &header);
        let mut expected: String = String::new();
        expected.push_str("ac76007450aeacb3d88bc485d4d86bb6300cc5e7c77fdf21dd4897".into());
        expected.push_str("fd90a843c5407547c754e65b98425eaa70c648528564f81ea65d7b".into());
        expected.push_str("459870032386ff0c1080c86ef561a5464a8f65a80a4a8b6580d370".into());
        expected.push_str("69755b7f3902b4dddbdd6f46cea4f741d09d2fd7e29f4a6c98e65a".into());
        expected.push_str("7027d04536bab66d9fe0720dfd18a247fed92568710f5d8c15149f".into());
        expected.push_str("cd49f9966dcfe9fc1acd01ff919e97d8a7e70dec0f4a027aacb752".into());
        expected.push_str("629c870bf96e76043ade98dd7e55b789a98a17bf0286fac73ea5b2".into());
        expected.push_str("cd9203c8b66df6b28e8a8b9c5263bafa9834d25880544ba745f7df".into());
        expected.push_str("2419eee1c0a262eb27976b519c135cbc57f3359f6840d52a64b72a".into());
        expected.push_str("fc403d058aaeb488a6952f587a530e3b7161a48b447c70fb94d1e4".into());

        let mut actual: String = String::new();
        for r in result {
            for i in 0..r.len() {
                actual.push_str(&format!("{:02x}", r[i]));
            }
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_get_indices_from_minimal() {
        let mut minimal: Vec<u8> = Vec::with_capacity(100);
        for i in 0..100 {
            minimal.push(i as u8);
        }

        let validator = EquihashValidator::new(210, 9);
        let result: Vec<i32> = validator.get_indices_from_minimal(minimal.as_slice(), 5);
        let expected: String = "0 0 4 2 0 48 16 5 1 32 28 8 2 0 0 15 4 1 4 18 4 49 16 21 5 33 28 \
                                24 6 1 0 31 8 2 4 34 8 50 16 37 9 34 28 40 10 2 0 47 12 3 4 50 12 \
                                51 16 53 13 35 28 56 14 3 0 63 16 4 5 2 16 52 17 5 17 36 29 8 18 \
                                4 1 15 20 5 5 18 20 53 17 21 21 37 29 24 22 5 1 31 24 6 5 34 0 0 \
                                0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 "
            .into();
        let mut actual: String = String::new();
        for r in &result {
            actual.push_str(&format!("{} ", r));
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_valid_solution_validate() {
        let minimal = "011ecfb957609b0c87104d3e8079bb5415d62e7093b7113e0e68d95303692f485b364e68fcd1c389a39e4ca906d64bc553772ac2ff636726f9effc69b8b035b5a5e21820c3a0eaa931b86ebe971a009fbf0842620d0e44330533b238ead0ce287fd55b38bc3287b2a4d246d5398b267b2e44525a1e6139e6a4453eefc57f7da8093d2a8322840e231a45bfe53e2e452cd30240a38dbb05f83f0952a27bfd6398ea4b77cd68ca90af82774cf1fc372818026637ca86e9334ffaba585daafa64078b7406b355ae301889c41e264e963d780936ba8ec587f7c20f207aa906e12da7b5a110cd9e540f22863d48705c4845f5ec6c08f7f95ee4b1e30aaa99a939eebfe62ab809a3a883480513b6969233795c629b111388f57b7756e5c779da537031dfd21d088458f3f4ce7e9fba990feb64d3f771dd3e467b6258aae3f0386d3f491f97195a54b55930cf7d3ffed525903da1fafb69d65a657baf212e98e27e842a07cc1027c1a6eb2c65621a173b06498592720fac88ca15580973f1f5690937dcab7f9c8bca36895abe32b2c8202963f9a36616272d98d62857d5166216bc5539ffaa53b211d4828662e2ab7bee6c48abcaae3874173e15260ac03ace94763bcca6b9811e59d1fb21f6d79c75d2cd135e673b55d62706f09e3b1c98dcdf7b6ae8893aa9b3317a2ec0ee037ec91ca10453ba65e498d997f1f0a10975fbfeac7758da8eab9b728714fba97c88a41ba86c020c26e49a9f5576eb58c1764769da25b29526a898294b4b809a93ca15cceed993155522263e14296ec635cce62e50aae0291a96e734ac2a3ee9cd4dfe9687b278ce593fd9f6df02a8f504b0b9afb2ce7adcc5cdfa18bc9e77134cc95cab2638e6efa6ae1e80da2b05f2cc6bee1356247e3ae33e24bec7311d8c735ce7249c1911eb3cbe2d14ae39153c47c829a7434417de37f98446821ddc5e701973232193e59266ef32f31ba44f4886f6583167eb6904b3428efca0c644ab8d1732bb227f8d78133729b078058b6ab514509f13f5563407bb13a16f931fe2fc3a9f1350e6e878eb70a438b442a51f7bdae56a7c77eb28d12c22e207f3cddd01fcbd6530a9c281876648e333a3e405aef2d4564abfdbad49b716dce5bd7b1d9a537f09b10bd3d61aad257844ac139f3c82ba99f78745ddafab7007e771965f4be732757876186cbcc221680a367e6ae1156ddeb26d79a4b5fa558358e0b7a83c6a5430f093d40c6508977676eb272838a20c77c07e1447c2d07b4a8c248d5b44e3e9bf2df20b94292daadbfb4871736dc91b13960bfe02394d0c3b1fd3960bb701c62ca93377e38f2724466be23c0870b446022c2f6679bba73a49a802fa11e6dfbf1972db45b9049b34afa2fbc7859a3de1a7772f1c2d98b3a4f16eaaae459d4ac2bd0b0bde7f74ee861e22ff63b9fda0bffa74c1240eb2c66f6cc9dcbc57816971265ff7480db06d273a9d1a47ac2729f98f3dc321058727638e6435725d7b1c4bc519540436c697fe6e082fdb791944d57f0c9f4e4a6d7005d898a73be03bd33b0ec477cc6f513fe33cb41c1011713f8bf4d731b864af197482483209add1bc2d821a5851220828ce1bed927906cc82c5624849d3e9e4ce777397d336adc242ff74380b7b7c68d5582c72eb02b8289ff37f18f583c4e56cfa0e54071a5187500732938e5553be2704e74297faae161988d0def98e69ad7c23f538e0a3d5bac5fef363461d0c6d4b0c95633b9e6ec84a24a7d4cd707e8f6b7fe6792e6e7ac0127a015cf2e41234ac9374787a637b761fb31ca4d0e78f03a61930560b4b38cfe3769bd30077f74046f2aded27d5d550f2ac906603eb09b34c6c74f7dc3d26c3189cf87b62b97c93cc3205279cdff6254d3289c7445a1fddba35b779ae16cb5d9002ee513be37633719a2928a319c458a5af29c96066065799da470d7c35f0973fbebf749dd6850c5a5e0b43ea095b489af2ba77e2d1"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;

        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_valid_solution_valid_solution() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_valid_solution_change_index() {
        let minimal = "013e126d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_valid_solution_swap_first_pair() {
        let minimal = "9b622804f83376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_valid_solution_swap_pair() {
        let minimal = "376139d74ac013e0e6d88a37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_valid_solution_swap_last_pairs() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b796504d25bc2137bff20a9ce"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_valid_solution_sorted_indies() {
        let minimal = "013e0c07577021f700a1ae02bb440bf64040b10175cf05e3a81dbfc0890cc22bc309aa0c2724a09cad42b5f50b39d0310db0c690832c310d714037a400e302c3d6aa0fa10840358100e1441772107d184683011ae084857f12e3404d93516afc45e05e1793d05e5ad17c8f867bd71a07606d62a1ba1f46f7761c552871bbe1c9b28742401d2adc75c971dfd1c7b36b1eefb47d0b41f7ca4858262225d88a47c2356cc8eb8c23ae48903262479009217e2497289425127158ca0dba2887a0a3809290e98a5a922a8974ac4862b3dc4af44c2d6040b5d1d2e0928b948c2ed198bd14a304780c66e7322b54cb90d331c60cdc79339a70d1ea434ded8d639035a3a4dd05f376138dd8db37b144deffc382988e303a39a1a0e6f663cb58cf4ae13d6834f808a3ee234fc565403301014a241212506d5641f1f10d8104413791117e44893d1332345709d17f18461fc119c134749651d9b4488bbd223ad4974f526a8e4b89d12fb934c1d6130a654cb21534c9e4d51fd38f8f4e526d3a43e4f01113e3c04fa7453eadf4fcf094058a51ba294b364532ad1535cb55ef915c02b574ed95ddcf5785215f40f58abd1637805a6739699fa5abfa56b0515b1d896d8865b959971cb45da5157f74560517d815a161263d859ab61785985ee261992187f4d620b11891c7631f0d8df0b63a2dd941346521099592d65b175988e966d5159c099673b8da461669493da61516a7db1adece6b958daf53f6beaf9aff236c791db32416d7b75b90006f050dbcb296fc535c111470f2a1c3e207106c5c65f1719c35c79be71ecf5ca6d172a159cda597396c1cf8f77410add08d77429fdd74ac75fff1dba8a771531de34777b319e1ef378f0c9e4e62794635e787f7a05d9f295a7dc835f7bef7e5a21fbd537f0a99fc3cd7fc0f5ffb86801f7a059358260e60a9ce83926e0fe31853da6169b285b77e18d568703f61d4f987947e1f4a887ebce1fbd187fca622f1e88e15e2483489e8462874e8cb3f6333678ce26e340048d0fb234f318d56e23567c8e8f923b40e8fba16420ba9102da44ba991692e466b891da9247b98927c1a4c12e93f36a5293b95377a54fec9559ca5773695eda65bc21970f4a5ee0998435261a189890ca627a399ecbe6a1ae9b622a6eda89bfb76721de9d5fba75f089ebeb67bd2f9f6e3e86221a3063a8e37ba42b0e94817a5587a9566fa5784a96451a59d7299d17a6ff3e9cd0ba741eaa1dfba8b696a2ee8a93b1aa6116aa34f6abac3ab6ee6aeb79acf39ab417cad8476b62a9adc48ab7791ae0e06ba79aaeda26bbf3caf2a56bf594b0ccc6c386eb143c2c5475b1ad9ecc0dbb33ed6ce2c1b3fa1ad21c7b49f32d2e37b4de26d3a7fb5c7eed8a8cb652dedc91bb7ff5ae0454b85f3ae9f79ba8d4eec976bb5cceeec48bbc58ef03cdbc740af8a52be6de6fa280bea23afb6fdbf0266fd17fbfa68302ef1c0e7a303b55c125970570bc1682f06796c277230a48cc330670d8d4c36c070effcc410a714fc2c58ceb166fbc5ad5f1b2d8c6e26b1e08ec7f64727e7fca3a7f296a2ca91d72a536cabecf2bb36cc4c2333fb5cd264f352b2cd71cb37f33ce64873d82bcfb88f40375d09a9b49082d246c74a524d2a41b4ac8ad38a67520a8d4e47f54ae8d5746b59030d668935c300d7cabf60705d89863636a9d9511369a35db0f4b6e9f6dbd24372889de3f8f7991cdf22b78218de0c41384a1fe1766386e85e2013b8ac36e2c32b8b3d0e2f6d78ede9e402af90143e5416f96018e60ef398de8e719b39cecae749eba0a2be82c5ba28bee915f7a5eb4ea1317a8bd7eb9a57b2a23ede9dbb7ca7efae7fbeef0eff867c1365f0a2b7cb9f0f36417d0181f426ffd11a1f5294bd56aaf69a33db150f6f31fdd3bef7564bddf8df81dd3e2453f8c72fe3f7df97a3be7b24fa308be919ffaab87eb690fd0213f5d22fd7fc3f620cfe0abff92c6ffd0fffff94"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }

    #[test]
    fn test_is_valid_solution_duplicate_index() {
        let minimal = "013e0c04f83376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }
    #[test]
    fn test_is_valid_solution_duplicate_first_half() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }
    #[test]
    fn test_is_valid_solution_invalid_solution_length() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c6"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }
    #[test]
    fn test_is_valid_solution() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, true);
    }
    #[test]
    fn test_is_valid_solution_solution_updated() {
        let minimal = "013e126d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 1u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }
    #[test]
    fn test_is_valid_solution_solution_invalid_nonce() {
        let minimal = "013e0e6d88a376139d74ac37417fb2a237429fe90ac31eefb6355b87715321fbd1a9845b1633adb0f4b7991c1ecdad4b36457d03fa28be8753e7c28add09a9b9505b296a49ae563a8b697db150331c63ae695339a720e49b09aa0d8c7c31215fec33317369669cd0b927c1aa4ec61c6ef8cb90d1f42d1222ef3cb58eba79a4445f95d3b61c9b2a476a46c791eccfb51d725e440b65b9599e1ef32836eb3130869493ed21c7461fc3a8bd7a38defbeb9f021f72721de91ee626fedd39bd9ba0a2b8d3cc7eaae1578522c386e5da515bc1438a1d3be919fa5205fda68c107d1ae045450162aeec4841f1f20fe31b49f3302ef117817a18d56ae0e078ac361f7ca73d82b91692e54dde02fd93b7ca7bf026785d9805d73e5293bb1ad9f8bdb52356cde3c323d2b85f9688347a90e303a49aa3bf620c0c690a3b40eb4e9ff636a93371e786e85986863983bc23ae4934c9e75fff35c3004e526e2385785b77ee17ce0286bb98de8100d60b948c2225db0570b4f8f03399212bd133e24536564b72a53644cc8e0593570f2a2007de076ff2b62a92291f100cc03047827db8f5fdd169566f319b9f78fe34052896c7629d5fbafb6fdb652df1fd910b39d1bf14d35a3a67bd2f6166aff408462471f1e08e1b58a9e81763d683695e12aa34f70d8d4b151d7ff43f100e14468302b3dc7a457d1d2adedffd693f36b8ede9105dc9699ce670265c41b144137be5e8ead05f37c8ad08af0ecc0dbb5c7ef296a234ded91d25965b177491b141b559ca6d1a741eaf03cd66d516b779177b31bbeef019ef5ebca9554d72ecfe86290e993eadf8e8f924c12e65210a3567c87fca7b7a767ef54f33fb5853da65c3d20de9031b2d83e022a248342d7477dbcc74fa747d09bf2479039ceca637c2ee9f794974f5ca8568d0fb2ed7331d09029bfcf7fc0f69561e6bd4fea1dfb8ce26edc91b44893fdd5927f0a9a75f085b6219c65f188bc7a6a1ae01d5deb611d240c9bcb9f02a89771b89a5c72d38b3d03ee235e518d87ebcebb68995dcdb05a0bbb25db3ee230fa10a62432d95113e7b244cb21632cfd6cc907e3f7d1a076227a111c552b84a1f719c3749082b4b8df09dc805e3ab9c66c37b146169b2d00dd74e299d640c3f82af16afc51d9b4912ea79014361263dc7b3d7396c2ce2c12e092b626184c29974a524412125815a15abfa7166fb2ed19b0db0163a2de67b2f45709e57b6955ef925ee0909c92b728895a67e98145f79e1ffe31cbb62a33bfe192d60412e2746a7db2fd17f6623a699d176d7b77d11a10c436d3a43e58abd1c3e2058de03d56aa61fd3703b551796b735c72c53f0b900ab1dfd1ff5ff04bee4ea2ee80ad7d6098394d51fd4cab45ac147fff94cdfccf607050d7142b3ce671e6fba84c528e02585ee246704e1c0fd1364d75391fcd4acb6e9f6ab6ee74a906d7cabfc136537636fe8c22778d1f2a4759bb6a2f1d029ebeb78804e02bb47039e81bddda420ba673b8efa88ec2923396018436042d3789d66893a5eb445fc615ddcfc9f9ff54ae80cb0c7cd9050f5aab55d1ac3bff3e0774f40607eb69023ae33104292f452b520a87fee1b16b57be294ba0b16040b104b8d027158df720d358e41a615174235e3336711ae0bf5d22a591472bb362497295c02b704452466b82b121aea353a3063ab7122488eb6aeb796bfc8f9d27a73e3de55672aefcf38b0ca8fba176f490bfa68369a350890cc9217e2509453c0446e4001e4e62989e8f04965619922abac3cd264f8218d620b11d042b7f0f36610d4322b57067966b7b3b831044fcf0985e1651ba29afabe3f1596340046f2ca654fec76ea2a1e51f87d2a328e9f09cad538f8f1793d2fa28039a1a130758a18886c50f021609bdd3be2887a2bf594691859f295abbc58fddf8d0e302f2afb317c8f9f7bef38298b4ac8ac33067f92c61ba1f7d4a52a59d72f9b7937bff20a9ce6504d25bc21"
            .from_hex().unwrap();
        let header = [0u8; 108];
        let mut nonce = [0u8; 32];
        nonce[0] = 9u8;
        let validator = EquihashValidator::new(210, 9);
        let result = validator.is_valid_solution(&minimal, &header, &nonce);
        assert_eq!(result, false);
    }
}
