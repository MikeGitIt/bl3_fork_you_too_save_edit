use bl3_save_edit_core::bl3_save::ammo::AmmoPool;

const PISTOL_TOTALS: [i32; 8] = [200, 300, 400, 500, 600, 700, 800, 900];
const SMG_TOTALS: [i32; 8] = [360, 540, 720, 900, 1080, 1260, 1440, 1620];
const AR_TOTALS: [i32; 8] = [280, 420, 560, 700, 840, 980, 1120, 1260];
const SNIPER_TOTALS: [i32; 8] = [50, 70, 90, 110, 130, 150, 170, 190];

fn totals_from_slice(slice: &'static [i32], level: i32) -> i32 {
    let max_index = (slice.len() as i32).saturating_sub(1);
    let clamped = level.clamp(0, max_index);
    slice[clamped as usize]
}

fn level_from_totals(slice: &'static [i32], total: i32) -> i32 {
    let mut closest_index = 0;
    let mut closest_diff = i32::MAX;
    for (index, value) in slice.iter().enumerate() {
        let diff = (value - total).abs();
        if diff < closest_diff {
            closest_diff = diff;
            closest_index = index as i32;
        }
    }
    closest_index
}

fn totals_for_key(key: &str) -> Option<&'static [i32]> {
    match key {
        "pistol" => Some(&PISTOL_TOTALS),
        "smg" => Some(&SMG_TOTALS),
        "assaultrifle" => Some(&AR_TOTALS),
        "sniper" => Some(&SNIPER_TOTALS),
        _ => None,
    }
}

pub fn bl4_level_to_total_for_key(key: &str, level: i32) -> i32 {
    totals_for_key(key)
        .map(|slice| totals_from_slice(slice, level))
        .unwrap_or(level)
}

pub fn bl4_total_to_level_for_key(key: &str, total: i32) -> i32 {
    totals_for_key(key)
        .map(|slice| level_from_totals(slice, total))
        .unwrap_or(total)
}

pub fn bl4_totals_for_pool(pool: &AmmoPool) -> Option<&'static [i32]> {
    match pool {
        AmmoPool::Pistol => Some(&PISTOL_TOTALS),
        AmmoPool::Smg => Some(&SMG_TOTALS),
        AmmoPool::Ar => Some(&AR_TOTALS),
        AmmoPool::Sniper => Some(&SNIPER_TOTALS),
        _ => None,
    }
}
