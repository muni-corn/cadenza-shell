export interface WttrReport {
  current_condition: CurrentCondition[];
  weather: Weather[];
}

interface CurrentCondition {
  humidity: string;
  FeelsLikeC: string;
  FeelsLikeF: string;
  observation_time: string;
  temp_C: string;
  temp_F: string;
  weatherCode: string;
  weatherDesc: { value: string }[];
}

interface Weather {
  astronomy: Astronomy[];
}

export interface Astronomy {
  sunrise: string;
  sunset: string;
}
