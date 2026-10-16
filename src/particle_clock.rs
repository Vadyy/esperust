pub fn emission_count(continuous: bool, rate: u32, old: f32, new: f32, start: f32, end: f32, carry: &mut f32) -> usize {
    if new <= start || old >= end { return 0; }
    if !continuous {
        if *carry < 0. { return 0; }
        *carry = -1.;
        return rate as usize;
    }
    *carry += (new.min(end) - old.max(start)).clamp(0., 0.25) * rate as f32;
    let count = carry.floor() as usize;
    *carry -= count as f32;
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_continuous_flash_fires_once_at_start_without_waiting_one_second() {
        let mut carry = 0.;
        assert_eq!(emission_count(false, 1, 0., 0.016, 0., 0.5, &mut carry), 1);
        assert_eq!(emission_count(false, 1, 0.016, 0.5, 0., 0.5, &mut carry), 0);
    }
    #[test]
    fn timed_emission_is_frame_rate_independent_and_stops_at_end() {
        for hz in [30., 60., 144.] {
            let mut carry = 0.; let mut count = 0;
            for i in 0..(hz as usize * 2) {
                count += emission_count(true, 100, i as f32 / hz, (i + 1) as f32 / hz, 0.2, 0.7, &mut carry);
            }
            assert!((49..=50).contains(&count), "{hz}: {count}");
        }
    }
}
