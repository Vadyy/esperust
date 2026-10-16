use bevy::prelude::Vec3;
use std::collections::HashMap;

const CELL_SIZE: f32 = 4.0;
const MAX_TRIANGLE_CELLS: i64 = 256;
const EPSILON: f32 = 0.000_001;

pub fn native_free_displacement(displacement: Vec3, axes: Vec3, skin: f32) -> Vec3 {
    if skin <= 0.0 {
        return displacement;
    }
    if !displacement.is_finite()
        || !axes.is_finite()
        || !skin.is_finite()
        || axes.min_element() <= 0.0
    {
        return Vec3::ZERO;
    }
    let length = (displacement / axes).length();
    if length <= skin {
        Vec3::ZERO
    } else {
        displacement * ((length - skin) / length)
    }
}

#[derive(Clone, Copy, Debug)]
struct Triangle {
    vertices: [Vec3; 3],
    normal: Vec3,
    min: Vec3,
    max: Vec3,
}

#[derive(Debug, Default)]
pub struct CollisionWorld {
    triangles: Vec<Triangle>,
    cells: HashMap<(i32, i32, i32), Vec<usize>>,
    large_triangles: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct MoveResult {
    pub position: Vec3,
    pub grounded: bool,
    pub wall_normal: Option<Vec3>,
    pub contact: Option<MapContact>,
    pub ceiling_hit: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MapContact {
    pub position: Vec3,
    pub normal: Vec3,
}

impl MapContact {
    pub fn walljump_normal(self, feet: Vec3, axes: Vec3) -> Option<Vec3> {
        let centre = feet + Vec3::Y * axes.y;
        if (centre.y - self.position.y).abs() >= 1.5 || self.normal.y.abs() >= 0.5 {
            return None;
        }
        let normal = Vec3::new(self.normal.x, 0.0, self.normal.z).normalize_or_zero();
        (normal != Vec3::ZERO).then_some(normal)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceHit {
    pub distance: f32,
    pub position: Vec3,
    pub normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct SphereHit {
    time: f32,
    point: Vec3,
    normal: Vec3,
    face_normal: Vec3,
}

#[derive(Default)]
struct EllipsoidContacts {
    contact: Option<MapContact>,
    floor: bool,
    wall: Option<Vec3>,
    ceiling: bool,
    zero_distance: bool,
}

impl EllipsoidContacts {
    fn merge(&mut self, other: Self) {
        self.contact = other.contact.or(self.contact);
        self.floor |= other.floor;
        self.wall = other.wall.or(self.wall);
        self.ceiling |= other.ceiling;
        self.zero_distance |= other.zero_distance;
    }
}

impl CollisionWorld {
    pub fn move_ellipsoid(&self, feet: Vec3, delta: Vec3, axes: Vec3, skin: f32) -> MoveResult {
        self.move_ellipsoid_with_free_skin(feet, delta, axes, skin, skin)
    }

    pub fn move_ellipsoid_with_free_skin(
        &self, feet: Vec3, delta: Vec3, axes: Vec3, skin: f32, free_skin: f32,
    ) -> MoveResult {
        let mut result = MoveResult {
            position: feet,
            grounded: false,
            wall_normal: None,
            contact: None,
            ceiling_hit: false,
        };
        if !feet.is_finite()
            || !delta.is_finite()
            || !axes.is_finite()
            || axes.min_element() <= 0.0
            || !skin.is_finite()
            || skin < 0.0
            || !free_skin.is_finite()
            || free_skin < 0.0
        {
            return result;
        }
        let centre = (feet + Vec3::Y * axes.y) / axes;
        let displacement = delta / axes;
        let (position, contacts) = if delta.y < 0.0 {
            let (horizontal, mut contacts) = self.slide_ellipsoid(
                centre,
                Vec3::new(displacement.x, 0.0, displacement.z),
                axes,
                skin,
                free_skin,
                false,
            );
            let (position, floor) =
                self.slide_ellipsoid(horizontal, Vec3::Y * displacement.y, axes, skin, free_skin, true);
            contacts.merge(floor);
            (position, contacts)
        } else {
            self.slide_ellipsoid(centre, displacement, axes, skin, free_skin, false)
        };
        result.position = position * axes - Vec3::Y * axes.y;
        if contacts.zero_distance
            && self.ellipsoid_overlaps(result.position, axes)
            && !self.ellipsoid_overlaps(feet, axes)
        {
            result.position = feet;
        }
        result.wall_normal = contacts.wall;
        result.contact = contacts.contact;
        result.ceiling_hit = contacts.ceiling;
        if delta.y <= 0.0 {
            result.grounded = contacts.floor || self.ellipsoid_support(position, axes, skin);
        }
        result
    }

    pub fn ellipsoid_overlaps(&self, feet: Vec3, axes: Vec3) -> bool {
        let centre = feet + Vec3::Y * axes.y;
        self.query(centre - axes, centre + axes)
            .into_iter()
            .any(|index| {
                let [a, b, c] = self.triangles[index].vertices.map(|v| (v - centre) / axes);
                closest_point_triangle(Vec3::ZERO, a, b, c).length_squared() < 1.0 - 0.00001
            })
    }

    fn ellipsoid_support(&self, centre: Vec3, axes: Vec3, skin: f32) -> bool {
        let delta = -Vec3::Y * (skin * 2.0 + 0.005);
        self.first_sphere_hit(centre, delta, axes)
            .is_some_and(|hit| {
                self.floor_contact_supported(centre + delta * hit.time, hit.point, axes, skin)
            })
    }

    fn floor_contact_supported(&self, centre: Vec3, point: Vec3, axes: Vec3, skin: f32) -> bool {
        if centre.y - point.y > 0.9 {
            return true;
        }
        let origin = Vec3::new(centre.x, point.y + skin, centre.z) * axes;
        self.raycast(origin, -Vec3::Y, axes.y * 0.7)
            .is_some_and(|gap| gap / axes.y < 0.7)
    }

    fn slide_ellipsoid(
        &self,
        mut position: Vec3,
        mut delta: Vec3,
        axes: Vec3,
        skin: f32,
        free_skin: f32,
        floor_pass: bool,
    ) -> (Vec3, EllipsoidContacts) {
        let mut contacts = EllipsoidContacts::default();
        let requested = delta;
        for depth in 0..5 {
            let length = delta.length();
            if length <= free_skin.max(EPSILON) {
                break;
            }
            let Some(hit) = self.first_sphere_hit(position, delta, axes) else {
                position += native_free_displacement(delta, Vec3::ONE, free_skin);
                break;
            };
            let direction = delta / length;
            let hit_distance = hit.time * length;
            let travel = (hit_distance - skin).max(0.0);
            let next = position + direction * travel;
            let world_normal = (hit.normal / axes).normalize_or_zero();
            contacts.contact = Some(MapContact {
                position: hit.point * axes,
                normal: (hit.normal * axes).normalize_or_zero(),
            });
            if hit_distance < 0.0001 {
                contacts.zero_distance = true;
                contacts.floor |= floor_pass;
                contacts.ceiling |= !floor_pass && world_normal.y < -0.5;
                if hit.face_normal.y.abs() < 0.5 || hit.normal.y < 0.9 {
                    contacts.wall = Some(world_normal);
                }
                break;
            }
            if floor_pass
                && (depth > 0 || self.floor_contact_supported(next, hit.point, axes, skin))
            {
                contacts.floor = true;
                position = next;
                break;
            } else if world_normal.y < -0.5 {
                contacts.ceiling = true;
            } else if hit.face_normal.y.abs() < 0.5 || hit.normal.y < 0.9 {
                contacts.wall = Some(world_normal);
            }
            let normal = (next - hit.point).normalize_or_zero();
            let destination = position + delta;
            let projected = destination - normal * (destination - hit.point).dot(normal);
            let residual = projected - hit.point;
            position = next;
            if (floor_pass && residual.y > EPSILON) || residual.dot(delta) <= 0.0 {
                break;
            }
            delta = residual;
        }
        if contacts.zero_distance {
            if floor_pass {
                position.y += skin;
            } else {
                let correction =
                    (requested * axes + Vec3::Y * axes.y).normalize_or_zero() * (skin * 0.01);
                position -= correction / axes;
            }
        }
        (position, contacts)
    }

    fn first_sphere_hit(&self, position: Vec3, delta: Vec3, axes: Vec3) -> Option<SphereHit> {
        let start = position * axes;
        let end = (position + delta) * axes;
        let mut nearest: Option<SphereHit> = None;
        for index in self.query(start.min(end) - axes, start.max(end) + axes) {
            let triangle = &self.triangles[index];
            let vertices = triangle.vertices.map(|v| v / axes);
            if let Some(mut hit) = swept_sphere_triangle(position, delta, vertices) {
                if nearest.is_none_or(|old| hit.time < old.time) {
                    hit.face_normal = triangle.normal;
                    nearest = Some(hit);
                }
            }
        }
        nearest
    }

    pub fn new(triangles: Vec<[Vec3; 3]>) -> Self {
        let mut world = Self::default();
        for vertices in triangles {
            if vertices.iter().any(|point| !point.is_finite()) {
                continue;
            }
            let cross = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
            if cross.length_squared() < EPSILON * EPSILON {
                continue;
            }
            let triangle = Triangle {
                vertices,
                normal: cross.normalize(),
                min: vertices[0].min(vertices[1]).min(vertices[2]),
                max: vertices[0].max(vertices[1]).max(vertices[2]),
            };
            let lo = cell(triangle.min);
            let hi = cell(triangle.max);
            let count = (i64::from(hi.0) - i64::from(lo.0) + 1)
                .saturating_mul(i64::from(hi.1) - i64::from(lo.1) + 1)
                .saturating_mul(i64::from(hi.2) - i64::from(lo.2) + 1);
            let index = world.triangles.len();
            world.triangles.push(triangle);
            if count > MAX_TRIANGLE_CELLS {
                world.large_triangles.push(index);
            } else {
                for x in lo.0..=hi.0 {
                    for y in lo.1..=hi.1 {
                        for z in lo.2..=hi.2 {
                            world.cells.entry((x, y, z)).or_default().push(index);
                        }
                    }
                }
            }
        }
        world
    }

    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        self.raycast_surface(origin, direction, max_distance).map(|hit| hit.distance)
    }

    pub fn raycast_surface(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<SurfaceHit> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !max_distance.is_finite()
            || max_distance < 0.0
            || direction.length_squared() < EPSILON * EPSILON
        {
            return None;
        }
        let direction = direction.normalize();
        let end = origin + direction * max_distance;
        let candidates = self.query(origin.min(end), origin.max(end));
        let mut nearest = max_distance;
        let mut normal = None;
        for index in candidates {
            if let Some(distance) = ray_triangle(origin, direction, &self.triangles[index]) {
                if distance >= 0.0 && distance <= nearest {
                    nearest = distance;
                    let n = self.triangles[index].normal;
                    normal = Some(if n.dot(direction) > 0. { -n } else { n });
                }
            }
        }
        normal.map(|normal| SurfaceHit { distance: nearest, position: origin + direction * nearest, normal })
    }

    fn query(&self, min: Vec3, max: Vec3) -> Vec<usize> {
        let lo = cell(min);
        let hi = cell(max);
        let count = (i64::from(hi.0) - i64::from(lo.0) + 1)
            .saturating_mul(i64::from(hi.1) - i64::from(lo.1) + 1)
            .saturating_mul(i64::from(hi.2) - i64::from(lo.2) + 1);
        if count > 65_536 {
            return self
                .triangles
                .iter()
                .enumerate()
                .filter_map(|(i, t)| bounds_overlap(min, max, t.min, t.max).then_some(i))
                .collect();
        }
        let mut indices = self.large_triangles.clone();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    if let Some(cell_indices) = self.cells.get(&(x, y, z)) {
                        indices.extend_from_slice(cell_indices);
                    }
                }
            }
        }
        indices.sort_unstable();
        indices.dedup();
        indices.retain(|&i| bounds_overlap(min, max, self.triangles[i].min, self.triangles[i].max));
        indices
    }
}

fn swept_sphere_triangle(position: Vec3, delta: Vec3, vertices: [Vec3; 3]) -> Option<SphereHit> {
    let [a, b, c] = vertices;
    let normal = (b - a).cross(c - a).normalize_or_zero();
    if normal == Vec3::ZERO {
        return None;
    }
    let mut nearest: Option<SphereHit> = None;
    let mut consider = |time: f32, point: Vec3| {
        if !(-EPSILON..=1.0).contains(&time) {
            return;
        }
        let time = time.max(0.0);
        let normal = (position + delta * time - point).normalize_or_zero();
        if normal.dot(delta) >= -EPSILON {
            return;
        }
        if nearest.is_none_or(|old| time < old.time) {
            nearest = Some(SphereHit {
                time,
                point,
                normal,
                face_normal: Vec3::ZERO,
            });
        }
    };
    let speed = delta.dot(normal);
    let distance = (position - a).dot(normal);
    if speed.abs() > EPSILON && distance.abs() >= 1.0 - EPSILON {
        for side in [-1.0_f32, 1.0] {
            let time = (side - distance) / speed;
            if (0.0..=1.0).contains(&time) {
                let point = position + delta * time - normal * side;
                if closest_point_triangle(point, a, b, c).distance_squared(point) < EPSILON * 16.0 {
                    consider(time, point);
                }
            }
        }
    }
    for vertex in vertices {
        let offset = position - vertex;
        if let Some(time) = first_root(
            delta.length_squared(),
            2.0 * delta.dot(offset),
            offset.length_squared() - 1.0,
        ) {
            consider(time, vertex);
        }
    }
    for (start, end) in [(a, b), (b, c), (c, a)] {
        let edge = end - start;
        let length = edge.length_squared();
        if length <= EPSILON {
            continue;
        }
        let offset = position - start;
        let perpendicular_delta = delta - edge * (delta.dot(edge) / length);
        let perpendicular_offset = offset - edge * (offset.dot(edge) / length);
        if let Some(time) = first_root(
            perpendicular_delta.length_squared(),
            2.0 * perpendicular_delta.dot(perpendicular_offset),
            perpendicular_offset.length_squared() - 1.0,
        ) {
            let fraction = (offset + delta * time).dot(edge) / length;
            if (0.0..=1.0).contains(&fraction) {
                consider(time, start + edge * fraction);
            }
        }
    }
    nearest
}

fn first_root(a: f32, b: f32, c: f32) -> Option<f32> {
    if a.abs() < EPSILON {
        return None;
    }
    let determinant = b * b - 4.0 * a * c;
    if determinant < 0.0 {
        return None;
    }
    for root in [
        (-b - determinant.sqrt()) / (2.0 * a),
        (-b + determinant.sqrt()) / (2.0 * a),
    ] {
        if root > 0.0 && root <= 1.0 {
            return Some(root);
        }
    }
    None
}

fn cell(point: Vec3) -> (i32, i32, i32) {
    let point = (point / CELL_SIZE).floor();
    (point.x as i32, point.y as i32, point.z as i32)
}

fn bounds_overlap(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    a_min.cmple(b_max).all() && a_max.cmpge(b_min).all()
}

fn closest_point_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denominator = 1.0 / (va + vb + vc);
    a + ab * (vb * denominator) + ac * (vc * denominator)
}

fn ray_triangle(origin: Vec3, direction: Vec3, triangle: &Triangle) -> Option<f32> {
    let [a, b, c] = triangle.vertices;
    let edge1 = b - a;
    let edge2 = c - a;
    let p = direction.cross(edge2);
    let determinant = edge1.dot(p);
    if determinant.abs() < EPSILON {
        return None;
    }
    let inverse = 1.0 / determinant;
    let t = origin - a;
    let u = t.dot(p) * inverse;
    if !(-EPSILON..=1.0 + EPSILON).contains(&u) {
        return None;
    }
    let q = t.cross(edge1);
    let v = direction.dot(q) * inverse;
    if v < -EPSILON || u + v > 1.0 + EPSILON {
        return None;
    }
    Some(edge2.dot(q) * inverse)
}

#[cfg(test)]
mod tests {
    use crate::content::simulation::parity_content;
    use super::*;

    #[test]
    fn swept_contact_is_cleared_on_departure_and_retains_world_contact_geometry() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let world = CollisionWorld::new(quad(
            Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 10.0, -10.0),
            Vec3::new(0.0, 10.0, 10.0), Vec3::new(0.0, 0.0, 10.0),
        ));
        let hit = world.move_ellipsoid(Vec3::new(-1.025, 2.0, 0.0), Vec3::X * 0.2, axes, 0.015);
        let contact = hit.contact.expect("Approach must produce contact");
        assert!(contact.position.x.abs() < 0.0001);
        assert!((contact.position.y - 3.6).abs() < 0.0001);
        assert_eq!(contact.walljump_normal(hit.position, axes), Some(Vec3::NEG_X));
        let departed = world.move_ellipsoid(hit.position, Vec3::new(-0.05, 0.4, 0.1), axes, 0.015);
        assert!(departed.contact.is_none());
        assert!(departed.position.x > -1.12, "Wall remains inside the former proximity allowance");
    }

    #[test]
    fn walljump_contact_height_and_normal_limits_are_strict() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let feet = -Vec3::Y * axes.y;
        let centre = Vec3::ZERO;
        for (height, allowed) in [(1.499, true), (1.5, false), (1.501, false), (-1.5, false)] {
            let contact = MapContact { position: centre + Vec3::Y * height, normal: Vec3::NEG_X };
            assert_eq!(contact.walljump_normal(feet, axes).is_some(), allowed, "height {height}");
        }
        for (y, allowed) in [(0.499_f32, true), (0.5, false), (-0.5, false)] {
            let normal = Vec3::new((1.0 - y*y).sqrt(), y, 0.0);
            let contact = MapContact { position: centre, normal };
            assert_eq!(contact.walljump_normal(feet, axes).is_some(), allowed, "normal Y {y}");
        }
    }

    #[test]
    fn native_free_skin_has_anisotropic_deadzone_and_one_backoff_per_move() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        assert_eq!(
            native_free_displacement(Vec3::X * 0.015, axes, 0.015),
            Vec3::ZERO
        );
        assert_eq!(
            native_free_displacement(Vec3::Y * 0.024, axes, 0.015),
            Vec3::ZERO
        );
        assert!(
            (native_free_displacement(Vec3::X * 0.176, axes, 0.015).x - 0.161).abs() < 0.000_001
        );
        assert!(
            (native_free_displacement(Vec3::Y * 0.4128, axes, 0.015).y - 0.3888).abs() < 0.000_001
        );
        let world = CollisionWorld::new(Vec::new());
        let moved = world.move_ellipsoid(Vec3::ZERO, Vec3::X * 10.0, axes, 0.015);
        assert!(
            (moved.position.x - 9.985).abs() < 0.0001,
            "padding must apply once to a free sweep"
        );
    }

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> Vec<[Vec3; 3]> {
        vec![[a, b, c], [a, c, d]]
    }

    fn floor(y: f32) -> Vec<[Vec3; 3]> {
        quad(
            Vec3::new(-20.0, y, -20.0),
            Vec3::new(20.0, y, -20.0),
            Vec3::new(20.0, y, 20.0),
            Vec3::new(-20.0, y, 20.0),
        )
    }

    #[test]
    fn smooth_free_margin_does_not_shrink_collision_clearance() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let mut triangles = floor(0.0);
        triangles.extend(wall(0.0, 10.0));
        let world = CollisionWorld::new(triangles);
        for hz in [60.0_f32, 120.0, 240.0, 1000.0] {
            let free_skin = 0.015 * 60.0 / hz;
            let landed = world.move_ellipsoid_with_free_skin(
                Vec3::new(-5.0, 4.0, 0.0), -Vec3::Y * 6.0, axes, 0.015, free_skin,
            );
            assert!(landed.grounded);
            assert!((landed.position.y - 0.024).abs() < 0.001, "{hz}: {landed:?}");
            let contact = world.move_ellipsoid_with_free_skin(
                landed.position, Vec3::X * 6.0, axes, 0.015, free_skin,
            );
            assert!((contact.position.x + 1.015).abs() < 0.001, "{hz}: {contact:?}");
            assert!(contact.grounded && contact.wall_normal.is_some());
            let ceiling = CollisionWorld::new(floor(6.0));
            let hit = ceiling.move_ellipsoid_with_free_skin(Vec3::ZERO, Vec3::Y * 8.0, axes, 0.015, free_skin);
            assert!(hit.ceiling_hit);
            assert!((hit.position.y - 2.776).abs() < 0.001, "{hz}: {hit:?}");
        }
    }

    #[test]
    fn original_ellipsoid_sweeps_floor_wall_ceiling_and_free_skin() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let mut triangles = floor(0.0);
        triangles.extend(wall(0.0, 10.0));
        let world = CollisionWorld::new(triangles);
        let landed = world.move_ellipsoid(Vec3::new(-5.0, 4.0, 0.0), -Vec3::Y * 6.0, axes, 0.015);
        assert!(landed.grounded, "{landed:?}");
        assert!((landed.position.y - 0.024).abs() < 0.001, "{landed:?}");
        let moved = world.move_ellipsoid(landed.position, Vec3::new(15.0, 0.0, 3.0), axes, 0.015);
        assert!(moved.position.x < -0.999, "{moved:?}");
        assert!(moved.position.z > 2.9, "{moved:?}");
        assert!(moved.grounded && moved.wall_normal.is_some(), "{moved:?}");
        let ceiling = CollisionWorld::new(floor(6.0));
        let hit = ceiling.move_ellipsoid(Vec3::ZERO, Vec3::Y * 8.0, axes, 0.015);
        assert!(hit.ceiling_hit && hit.position.y < 2.801, "{hit:?}");
        let free =
            CollisionWorld::default().move_ellipsoid(Vec3::ZERO, Vec3::X * 0.176, axes, 0.015);
        assert!((free.position.x - 0.161).abs() < 0.000001, "{free:?}");
    }

    #[test]
    fn original_ellipsoid_climbs_stair_edges_without_an_artificial_step_lift() {
        let axes = Vec3::new(1.0, 1.6, 1.0);
        let mut triangles = floor(0.0);
        for step in 0..8 {
            let x = step as f32 * 1.1;
            let y = (step + 1) as f32 * 0.4;
            triangles.extend(quad(
                Vec3::new(x, y - 0.4, -4.0),
                Vec3::new(x, y, -4.0),
                Vec3::new(x, y, 4.0),
                Vec3::new(x, y - 0.4, 4.0),
            ));
            triangles.extend(quad(
                Vec3::new(x, y, -4.0),
                Vec3::new(x + 1.1, y, -4.0),
                Vec3::new(x + 1.1, y, 4.0),
                Vec3::new(x, y, 4.0),
            ));
        }
        let world = CollisionWorld::new(triangles);
        let mut feet = Vec3::new(-2.0, 0.024, 0.0);
        for _ in 0..70 {
            let planar = world.move_ellipsoid(feet, Vec3::X * 0.176, axes, 0.015);
            let down = world.move_ellipsoid(planar.position, -Vec3::Y * 0.12, axes, 0.015);
            feet = down.position;
        }
        assert!(feet.x > 7.0 && feet.y > 3.0, "blocked on stairs: {feet:?}");
    }

    #[test]
    fn original_circle_ring_side_cannot_support_a_half_climb() {
        let map = crate::scene::read_map_for(crate::scene::MapId::Circle2).unwrap();
        let level = crate::scene::make_level(&map);
        let axes = Vec3::new(1.0, 1.6, 1.0);
        for direction in [-1.0_f32, 1.0] {
            let mut feet = Vec3::new(2.0, -4.8, if direction < 0.0 { -18.0 } else { -21.5 });
            let mut velocity = -7.5_f32;
            for _ in 0..200 {
                let planar =
                    level
                        .collision
                        .move_ellipsoid(feet, Vec3::Z * direction * 0.176, axes, 0.015);
                velocity = (velocity - 75.0 * 0.016).max(-75.0);
                let down = level.collision.move_ellipsoid(
                    planar.position,
                    Vec3::Y * velocity * 0.016,
                    axes,
                    0.015,
                );
                feet = down.position;
                if down.grounded {
                    velocity = 0.0;
                }
            }
            assert!(feet.y < -8.6, "half-climbed direction{direction}: {feet:?}");
        }
    }

    #[test]
    fn imported_station2_stairs_are_walkable_in_both_directions() {
        let map = crate::scene::read_map_for(crate::scene::MapId::Station2).unwrap();
        let level = crate::scene::make_level(&map);
        let axes = Vec3::new(1.0, 1.6, 1.0);
        for side in [-1.0_f32, 1.0] {
            let mut feet = Vec3::new(side * 16.0, 11.0, -side * 13.0);
            feet = level
                .collision
                .move_ellipsoid(feet, -Vec3::Y * 4.0, axes, 0.015)
                .position;
            let start = feet;
            for _ in 0..110 {
                let planar =
                    level
                        .collision
                        .move_ellipsoid(feet, -Vec3::Z * side * 0.176, axes, 0.015);
                feet = level
                    .collision
                    .move_ellipsoid(planar.position, -Vec3::Y * 0.12, axes, 0.015)
                    .position;
            }
            assert!(
                (feet.z - start.z).abs() > 10.0 && feet.y > start.y + 7.0,
                "stair ascent {side}: {start:?} -> {feet:?}"
            );
            let top = feet;
            for _ in 0..110 {
                let planar =
                    level
                        .collision
                        .move_ellipsoid(feet, Vec3::Z * side * 0.176, axes, 0.015);
                feet = level
                    .collision
                    .move_ellipsoid(planar.position, -Vec3::Y * 0.4, axes, 0.015)
                    .position;
            }
            assert!(
                feet.y < top.y - 6.5,
                "stair descent {side}: {top:?} -> {feet:?}"
            );
        }
    }

    #[test]
    fn shared_gameplay_walks_original_stairs_and_falls_off_ring_sides() {
        use crate::gameplay::{Motion, MovementInput, Player};
        let motion = Motion::default();
        let dt = 0.016;
        let station = crate::scene::make_level(
            &crate::scene::read_map_for(crate::scene::MapId::Station2).unwrap(),
        );
        for side in [-1.0_f32, 1.0] {
            let mut player = Player::new(Vec3::new(side * 16.0, 11.0, -side * 13.0), &motion);
            for _ in 0..60 {
                player.step(parity_content(), &station.collision, &motion, MovementInput::default(), dt);
            }
            let start = player.feet;
            for _ in 0..110 {
                player.step(parity_content(), &station.collision,
                    &motion,
                    MovementInput {
                        movement: bevy::prelude::Vec2::Y,
                        yaw: if side > 0.0 {
                            0.0
                        } else {
                            std::f32::consts::PI
                        },
                        ..Default::default()
                    },
                    dt,
                );
            }
            assert!(
                player.feet.y > start.y + 7.0 && (player.feet.z - start.z).abs() > 10.0,
                "shared stair ascent {side}: {start:?} -> {:?}",
                player.feet
            );
        }
        let circle = crate::scene::make_level(
            &crate::scene::read_map_for(crate::scene::MapId::Circle2).unwrap(),
        );
        for direction in [-1.0_f32, 1.0] {
            let mut player = Player::new(
                Vec3::new(2.0, -4.8, if direction < 0.0 { -18.0 } else { -21.5 }),
                &motion,
            );
            for _ in 0..180 {
                player.step(parity_content(), &circle.collision,
                    &motion,
                    MovementInput {
                        movement: bevy::prelude::Vec2::Y,
                        yaw: if direction < 0.0 {
                            0.0
                        } else {
                            std::f32::consts::PI
                        },
                        ..Default::default()
                    },
                    dt,
                );
            }
            assert!(
                player.feet.y < -8.6 && player.grounded,
                "shared ring side {direction}: {:?}",
                player.feet
            );
        }
    }

    fn wall(x: f32, top: f32) -> Vec<[Vec3; 3]> {
        quad(
            Vec3::new(x, 0.0, -20.0),
            Vec3::new(x, top, -20.0),
            Vec3::new(x, top, 20.0),
            Vec3::new(x, 0.0, 20.0),
        )
    }

    #[test]
    fn raycast_hits_nearest_face_from_either_side() {
        let mut triangles = wall(0.0, 4.0);
        triangles.extend(wall(2.0, 4.0));
        let world = CollisionWorld::new(triangles);
        assert!(
            (world
                .raycast(Vec3::new(-3.0, 1.0, 0.0), Vec3::X * 2.0, 10.0)
                .unwrap()
                - 3.0)
                .abs()
                < EPSILON
        );
        assert!(
            (world
                .raycast(Vec3::new(3.0, 1.0, 0.0), -Vec3::X, 10.0)
                .unwrap()
                - 1.0)
                .abs()
                < EPSILON
        );
        assert!(
            world
                .raycast(Vec3::new(-3.0, 1.0, 0.0), -Vec3::X, 10.0)
                .is_none()
        );
    }
}
