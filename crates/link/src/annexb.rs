pub const START_CODE: [u8; 4] = [0, 0, 0, 1];

pub mod nal {
    pub const IDR: u8 = 5;
    pub const SPS: u8 = 7;
    pub const PPS: u8 = 8;
}

pub fn units(access_unit: &[u8]) -> impl Iterator<Item = &[u8]> {
    let starts: Vec<usize> =
        access_unit.windows(START_CODE.len()).enumerate().filter_map(|(i, w)| (w == START_CODE).then_some(i)).collect();
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([access_unit.len()]).collect();
    starts.into_iter().zip(ends).map(move |(start, end)| &access_unit[start..end])
}

pub fn unit_type(unit: &[u8]) -> Option<u8> {
    unit.get(START_CODE.len()).map(|header| header & 0x1f)
}

pub fn parameter_sets(access_unit: &[u8]) -> Option<(&[u8], &[u8])> {
    let find = |kind| units(access_unit).find(|u| unit_type(u) == Some(kind));
    Some((find(nal::SPS)?, find(nal::PPS)?))
}

pub fn is_decodable_keyframe(access_unit: &[u8]) -> bool {
    parameter_sets(access_unit).is_some() && units(access_unit).any(|u| unit_type(u) == Some(nal::IDR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(kind: u8, body: &[u8]) -> Vec<u8> {
        [&START_CODE[..], &[0x60 | kind], body].concat()
    }

    #[test]
    fn splits_units_at_start_codes() {
        let au = [unit(nal::SPS, &[1, 2]), unit(nal::PPS, &[3]), unit(nal::IDR, &[4, 5, 6])].concat();
        let got: Vec<Option<u8>> = units(&au).map(unit_type).collect();
        assert_eq!(got, [Some(nal::SPS), Some(nal::PPS), Some(nal::IDR)]);
        assert_eq!(units(&au).map(<[u8]>::len).sum::<usize>(), au.len());
    }

    #[test]
    fn finds_parameter_sets_with_their_start_codes() {
        let sps = unit(nal::SPS, &[0x42, 0, 0x1f]);
        let pps = unit(nal::PPS, &[0xce]);
        let au = [sps.clone(), pps.clone(), unit(nal::IDR, &[9; 20])].concat();
        assert_eq!(parameter_sets(&au), Some((&sps[..], &pps[..])));
        assert!(is_decodable_keyframe(&au));
    }

    #[test]
    fn rejects_frames_that_cannot_start_a_decoder() {
        let delta = unit(1, &[7; 10]);
        let idr_without_sps = [unit(nal::PPS, &[1]), unit(nal::IDR, &[2])].concat();
        for au in [&[][..], &[0, 0, 0][..], &START_CODE[..], &delta[..], &idr_without_sps[..]] {
            assert!(!is_decodable_keyframe(au), "{au:?}");
        }
    }
}
