use sanitize_filename::sanitize;

use std::path::Path;

pub fn combine_path(path: &Path, folder: &str, url: &str) -> String {
    let file_name = sanitize(url.split('/').last().unwrap_or(""));
    let combined_path = path.join(sanitize(folder)).join(file_name);
    combined_path.to_string_lossy().to_string()
}

fn parse_resolution(res: &str) -> (u32, u32) {
    let parts: Vec<&str> = res.split('x').collect();
    let width = parts[0].parse::<u32>().unwrap();
    let height = parts[1].parse::<u32>().unwrap();
    (width, height)
}

fn calculate_distance(res1: (u32, u32), res2: (u32, u32)) -> u32 {
    let width_diff = (res1.0 as i32 - res2.0 as i32).abs() as u32;
    let height_diff = (res1.1 as i32 - res2.1 as i32).abs() as u32;
    width_diff + height_diff
}

pub fn closest_resolution(res_list: &[&String], target_res: &str) -> String {
    let target = parse_resolution(target_res);
    let mut closest_res = res_list[0].to_owned();
    let mut closest_distance = calculate_distance(parse_resolution(&closest_res), target);

    for res in res_list.iter().skip(1).into_iter() {
        let current_distance = calculate_distance(parse_resolution(&(*res).clone()), target);
        if current_distance < closest_distance {
            closest_res = res.to_string();
            closest_distance = current_distance;
        }
    }

    closest_res.to_string()
}
