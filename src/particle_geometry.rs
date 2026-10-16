use bevy::prelude::*;

pub fn variance(random: &mut impl FnMut() -> f32, amount: f32) -> f32 {
    (random() - random()) * amount
}

pub fn volume(kind: u32, min_width: f32, max_width: f32, min_height: f32, max_height: f32, random: &mut impl FnMut() -> f32) -> Vec3 {
    let width = min_width + (max_width - min_width) * random();
    let height = min_height + (max_height - min_height) * random();
    let a = random() * 6.28;
    if kind == 0 {
        let b = random() * 6.28;
        Vec3::new(b.sin() * width * a.cos(), b.cos() * height, b.sin() * width * a.sin())
    } else {
        Vec3::new(a.cos() * width, height, a.sin() * width)
    }
}

pub fn velocity(direction: Vec3, speed: f32, cone_degrees: f32, random: &mut impl FnMut() -> f32) -> Vec3 {
    let mut value = direction.normalize_or(Vec3::Z) * speed;
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        value = Quat::from_axis_angle(axis, variance(random, cone_degrees).to_radians()) * value;
    }
    value
}

pub fn integrate(position: &mut Vec3, velocity: &mut Vec3, gravity: Vec3, center: Vec3, attractive: f32, dt: f32) {
    *position += *velocity * dt;
    *velocity += gravity * dt;
    *velocity += (center - *position).normalize_or_zero() * attractive * dt;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_burst_starts_at_common_center_and_spreads_on_all_axes() {
        let mut state = 1234_u32;
        let mut random = || { state ^= state << 13; state ^= state >> 17; state ^= state << 5; (state >> 8) as f32 / 16_777_216. };
        let mut low = Vec3::splat(f32::MAX); let mut high = Vec3::splat(f32::MIN);
        for _ in 0..100 {
            assert_eq!(volume(0, 0., 0., 0., 0., &mut random), Vec3::ZERO);
            let value = velocity(Vec3::Z, 500., 360., &mut random);
            assert!((value.length() - 500.).abs() < 0.001);
            low = low.min(value); high = high.max(value);
        }
        assert!(low.cmplt(Vec3::splat(-200.)).all() && high.cmpgt(Vec3::splat(200.)).all());
    }
    #[test]
    fn shell_emission_uses_minimum_radius_and_attraction_moves_inward() {
        let point = volume(0, 200., 200., 200., 200., &mut || 0.25);
        assert!((point.length() - 200.).abs() < 0.001);
        let mut position = point; let mut velocity = Vec3::ZERO;
        integrate(&mut position, &mut velocity, Vec3::ZERO, Vec3::ZERO, 800., 0.1);
        assert!(velocity.dot(point) < 0.);
        integrate(&mut position, &mut velocity, Vec3::ZERO, Vec3::ZERO, 800., 0.1);
        assert!(position.length() < point.length());
    }
}
