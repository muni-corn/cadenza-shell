use serde::Deserialize;

#[derive(Debug, Default, Clone)]
pub struct WeatherState {
    pub temperature: i32,
    pub condition: String,
    pub icon: String,
}

#[derive(Deserialize, Debug)]
pub struct WttrDesc {
    pub value: String,
}

#[derive(Deserialize, Debug)]
pub struct WttrCondition {
    #[serde(rename = "temp_F")]
    pub temp_f: String,
    #[serde(rename = "weatherCode")]
    pub weather_code: String,
    #[serde(rename = "weatherDesc")]
    pub weather_desc: Vec<WttrDesc>,
}

#[derive(Deserialize, Debug)]
pub struct WttrAstronomy {
    pub sunrise: String,
    pub sunset: String,
}

#[derive(Deserialize, Debug)]
pub struct WttrDay {
    pub astronomy: Vec<WttrAstronomy>,
}

#[derive(Deserialize, Debug)]
pub struct WttrReport {
    pub current_condition: Vec<WttrCondition>,
    pub weather: Vec<WttrDay>,
}
