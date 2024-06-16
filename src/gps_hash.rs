fn interleave_u32_with_zeros(input: u32) -> u64 {
    let mut output: u64 = input.into();
    output = (output ^ (output << 16)) & 0x0000ffff0000ffff;
    output = (output ^ (output << 8)) & 0x00ff00ff00ff00ff;
    output = (output ^ (output << 4)) & 0x0f0f0f0f0f0f0f0f;
    output = (output ^ (output << 2)) & 0x3333333333333333;
    output = (output ^ (output << 1)) & 0x5555555555555555;
    return output;
}

fn get_adjusted_coords(lat: f32, lon: f32) -> (u32, u32) {
    let lat_adj: u32 = ((lat + 90.0) * 100000.0).trunc() as u32 * 2;
    let lon_adj: u32 = ((lon + 180.0) * 100000.0).trunc() as u32;

    (lat_adj, lon_adj)
}

pub fn get_gps_coords_hash(lat: f32, lon: f32) -> u64 {
    let (lat_adj, lon_adj) = get_adjusted_coords(lat, lon);

    (interleave_u32_with_zeros(lat_adj) << 1) | interleave_u32_with_zeros(lon_adj)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct InterleaveTest {
        input: u32,
        output: u64,
    }

    struct HashTest {
        lat: f32,
        lon: f32,
        output: u64,
    }

    #[test]
    fn interleave_works() {
        let tests = vec![
            InterleaveTest {
                input: 0b11010011001110100100010000110000,
                output: 0b101000100000101000001010100010000010000000100000000010100000000,
            },
            InterleaveTest {
                input: 0b01001001100001110000001000110010,
                output: 0b001000001000001010000000001010100000000000001000000010100000100,
            },
        ];

        for test_case in tests {
            let result = interleave_u32_with_zeros(test_case.input);
            assert_eq!(result, test_case.output);
        }
    }

    #[test]
    fn coords_adj_works() {
        let tests = vec![
            (57.153232, 24.858824, 29430646, 20485882),
            (-77.499, -69.500, 2500200, 11050000),
        ];

        for (input_lat, input_lon, output_lat, output_lon) in tests {
            let (lat_adj, lon_adj) = get_adjusted_coords(input_lat, input_lon);
            assert_eq!(lat_adj, output_lat);
            assert_eq!(lon_adj, output_lon);
        }
    }

    #[test]
    fn hash_works() {
        let tests = vec![
            HashTest {
                lat: 57.153232, // 0  1  1  0  0  1  1  1  1  0  0  1  1  0  1  1  1  1  0  1  0  1  0  0  0
                lon: 24.858824, // 1  0  0  1  1  1  0  0  0  1  0  0  1  0  1  1  0  1  1  1  1  1  0  1  0
                output: 0b01101001011110101001001011001111101101110111000100,
            },
            HashTest {
                lat: -77.499, // 0  0  1  0  0  1  1  0  0  0  1  0  0  1  1  0  0  1  1  0  1  0  0  0
                lon: -69.500, // 1  0  1  0  1  0  0  0  1  0  0  1  1  1  0  0  0  0  0  1  0  0  0  0
                output: 0b010011000110100001001001011110000010100110000000,
            },
        ];

        for test in tests {
            let output = get_gps_coords_hash(test.lat, test.lon);
            println!("ex {:b}", test.output);
            println!("re {:b}", output);
            assert_eq!(output, test.output);
        }
    }
}
