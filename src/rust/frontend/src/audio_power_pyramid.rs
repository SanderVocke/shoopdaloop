#[derive(Default)]
pub struct AudioPowerPyramidLevelData {
    pub subsampling_factor : usize,
    pub data : Vec<f32>
}

#[derive(Default)]
pub struct AudioPowerPyramidData {
    pub levels : Vec<AudioPowerPyramidLevelData>,
}

pub fn create_audio_power_pyramid<It>(input_data : It, n_levels : usize) -> AudioPowerPyramidData
    where It : Iterator<Item = f64>
{
    let mut current_level_power : Vec<f32> = input_data.map(
        |v| {
            // Abs and clamp
            let v = f32::max(v.abs() as f32, 10_f32.powf(-45_f32));
            // to dB
            let v = 20_f32 * v.log(10_f32);
            // To rendering range (0-1)
            let v = f32::max(1_f32 - (-v)/45_f32, 0_f32);
            v
        }
    ).collect();
    let n_input_samples = current_level_power.len();

    let mut rval = AudioPowerPyramidData {
        levels : Vec::with_capacity(n_levels.try_into().unwrap()),
    };

    let mut current_size = n_input_samples;
    for i in 0..n_levels {
        if i>0 && current_size > 1 {
            // Reduce by 2
            let prev_power : Vec<f32> = current_level_power;
            current_size /= 2;
            current_level_power = Vec::with_capacity(current_size);
            for j in 0..current_size {
                current_level_power.push(f32::max(prev_power[2*j], prev_power[2*j+1]));
            }
        }
        let level = AudioPowerPyramidLevelData {
            subsampling_factor : if current_size > 0 && n_input_samples > 0 { n_input_samples / current_size } else {1},
            data : current_level_power.clone(),
        };
        rval.levels.push(level);
    }

    rval
}