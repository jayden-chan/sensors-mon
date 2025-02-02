use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};

use anyhow::Result;
use lm_sensors::{Initializer, LMSensors};
use num_format::{Locale, ToFormattedString};
use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols,
    text::Span,
    widgets::{
        Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, LegendPosition,
        Padding, Row, Table,
    },
    DefaultTerminal, Frame,
};

const INTERVAL: u64 = 2000;
const WINDOW_SIZE: u64 = (5 * 60) / (INTERVAL / 1000);
const BOUNDS_PADDING: f64 = 2.0;
const BOUNDS_MIN: f64 = 25.0;
const BOUNDS_MAX: f64 = 90.0;

const B_TO_MIB: u64 = 1024 * 1024;

const CPU_CTL_LABEL: &str = "7800 X3D CTL";
const CPU_CCD_LABEL: &str = "7800 X3D CCD";
const COOLANT_1_LABEL: &str = "Coolant 1";
const COOLANT_2_LABEL: &str = "Coolant 2";
const GPU_LABEL: &str = "RTX 4070";

#[derive(Debug)]
struct LmSensorsValues {
    tctl: f64,
    tccd1: f64,
    coolant1: f64,
    coolant2: f64,
}

#[derive(Debug)]
struct NvmlValues {
    temp: f64,
    watts: f64,
    mem_used: u64,
    mem_total: u64,
}

fn get_nvml_values(nvml: &Nvml) -> NvmlValues {
    let mut temp: f64 = 0.0;
    let mut watts: f64 = 0.0;
    let mut mem_used: u64 = 0;
    let mut mem_total: u64 = 0;

    if let Ok(device) = nvml.device_by_index(0) {
        if let Ok(c) = device.temperature(TemperatureSensor::Gpu) {
            temp = c as f64;
        }

        if let Ok(mw) = device.power_usage() {
            watts = mw as f64 / 1000.0;
        }

        if let Ok(mem_info) = device.memory_info() {
            mem_used = mem_info.used / B_TO_MIB;
            mem_total = mem_info.total / B_TO_MIB;
        }
    }

    NvmlValues {
        temp,
        watts,
        mem_used,
        mem_total,
    }
}

fn get_lmsensors_vals(sensors: &LMSensors) -> LmSensorsValues {
    let mut tctl: f64 = 0.0;
    let mut tccd1: f64 = 0.0;
    let mut coolant1: f64 = 0.0;
    let mut coolant2: f64 = 0.0;

    for chip in sensors.chip_iter(None) {
        let cname = chip.name();
        let cname = cname.as_deref().unwrap_or("");
        if cname.starts_with("quadro-hid-") || cname == "k10temp-pci-00c3" {
            for feature in chip.feature_iter() {
                let name = feature.name().unwrap_or(Ok("")).unwrap_or("");

                if let fname @ ("temp1" | "temp2" | "temp3") = name {
                    for sub_feature in feature.sub_feature_iter() {
                        let sname =
                            sub_feature.name().unwrap_or(Ok("")).unwrap_or("");

                        if !sname.ends_with("_input") {
                            continue;
                        }

                        if let Ok(lm_sensors::Value::TemperatureInput(t)) =
                            sub_feature.value()
                        {
                            if cname.starts_with("quadro-hid-") {
                                match fname {
                                    "temp1" => coolant1 = t,
                                    "temp2" => coolant2 = t,
                                    _ => {}
                                }
                            } else {
                                match fname {
                                    "temp1" => tctl = t,
                                    "temp3" => tccd1 = t,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    LmSensorsValues {
        tctl,
        tccd1,
        coolant1,
        coolant2,
    }
}

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let app_result = App::new().run(terminal);
    ratatui::restore();
    app_result
}

struct App {
    sensors: LMSensors,
    nvml: Nvml,
    tctl: Vec<(f64, f64)>,
    tctl_mm: (f64, f64),
    tccd1: f64,
    tccd1_mm: (f64, f64),
    coolant1: Vec<(f64, f64)>,
    coolant1_mm: (f64, f64),
    coolant2: f64,
    coolant2_mm: (f64, f64),
    gpu_temp: Vec<(f64, f64)>,
    gpu_temp_mm: (f64, f64),
    gpu_w: f64,
    gpu_mem_used: u64,
    gpu_mem_max: u64,
    window: [f64; 2],
}

impl App {
    fn new() -> Self {
        let sensors: LMSensors = Initializer::default()
            .initialize()
            .expect("Failed to init lm-sensors");

        let nvml = Nvml::init().expect("Failed to initialize NVML");

        let mut tctl = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut coolant1 = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut gpu = Vec::with_capacity(WINDOW_SIZE as usize);

        for i in 0..(WINDOW_SIZE - 1) {
            tctl.push((i as f64, 0.0));
            coolant1.push((i as f64, 0.0));
            gpu.push((i as f64, 0.0));
        }

        let values = get_lmsensors_vals(&sensors);
        tctl.push(((WINDOW_SIZE - 1) as f64, values.tctl));
        coolant1.push(((WINDOW_SIZE - 1) as f64, values.coolant1));

        let nvml_values = get_nvml_values(&nvml);
        let gpu_temp = nvml_values.temp;
        gpu.push(((WINDOW_SIZE - 1) as f64, gpu_temp));

        Self {
            sensors,
            nvml,
            tctl,
            tctl_mm: (values.tctl, values.tctl),
            tccd1: values.tccd1,
            tccd1_mm: (values.tccd1, values.tccd1),
            coolant1,
            coolant1_mm: (values.coolant1, values.coolant1),
            coolant2: values.coolant2,
            coolant2_mm: (values.coolant2, values.coolant2),
            gpu_temp: gpu,
            gpu_temp_mm: (gpu_temp, gpu_temp),
            gpu_w: nvml_values.watts,
            gpu_mem_used: nvml_values.mem_used,
            gpu_mem_max: nvml_values.mem_total,
            window: [0.0, WINDOW_SIZE as f64],
        }
    }

    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let tick_rate = Duration::from_millis(INTERVAL);
        let mut last_tick = Instant::now();
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                }
            }
            if last_tick.elapsed() >= tick_rate {
                self.on_tick();
                last_tick = Instant::now();
            }
        }
    }

    fn on_tick(&mut self) {
        let vals = get_lmsensors_vals(&self.sensors);
        let nvml_vals = get_nvml_values(&self.nvml);

        self.window[0] += 1.0;
        self.window[1] += 1.0;

        let w = self.window[1];

        self.tctl.remove(0);
        self.coolant1.remove(0);
        self.gpu_temp.remove(0);

        self.tctl.push((w, vals.tctl));
        self.coolant1.push((w, vals.coolant1));
        self.gpu_temp.push((w, nvml_vals.temp));

        self.tccd1 = vals.tccd1;
        self.coolant2 = vals.coolant2;
        self.gpu_w = nvml_vals.watts;
        self.gpu_mem_used = nvml_vals.mem_used;
        self.gpu_mem_max = nvml_vals.mem_total;

        if vals.tctl < self.tctl_mm.0 {
            self.tctl_mm.0 = vals.tctl
        }
        if vals.tctl > self.tctl_mm.1 {
            self.tctl_mm.1 = vals.tctl
        }
        if vals.tccd1 < self.tccd1_mm.0 {
            self.tccd1_mm.0 = vals.tccd1
        }
        if vals.tccd1 > self.tccd1_mm.1 {
            self.tccd1_mm.1 = vals.tccd1
        }
        if vals.coolant1 < self.coolant1_mm.0 {
            self.coolant1_mm.0 = vals.coolant1
        }
        if vals.coolant1 > self.coolant1_mm.1 {
            self.coolant1_mm.1 = vals.coolant1
        }
        if vals.coolant2 < self.coolant2_mm.0 {
            self.coolant2_mm.0 = vals.coolant2
        }
        if vals.coolant2 > self.coolant2_mm.1 {
            self.coolant2_mm.1 = vals.coolant2
        }
        if nvml_vals.temp < self.gpu_temp_mm.0 {
            self.gpu_temp_mm.0 = nvml_vals.temp
        }
        if nvml_vals.temp > self.gpu_temp_mm.1 {
            self.gpu_temp_mm.1 = nvml_vals.temp
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(9)])
                .areas(frame.area());

        let [bottom_left, bottom_right] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(34)])
                .areas(bottom);

        let [bottom_left_1, bottom_left_2, bottom_left_3, bottom_left_4] =
            Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(3),
            ])
            .areas(bottom_left);

        self.render_temps_chart(frame, top);
        self.render_temps_table(frame, bottom_right);

        let c1 = self.coolant1.last().unwrap().1;
        let b1 = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .padding(Padding::new(0, 0, 1, 0));

        let c2 = self.coolant1.last().unwrap().1;
        let b2 = Block::default()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .title(COOLANT_2_LABEL);

        self.render_coolant_gauge(c1, b1, frame, bottom_left_1);
        self.render_coolant_gauge(c2, b2, frame, bottom_left_2);

        self.render_gpu_watts_gauge(self.gpu_w, frame, bottom_left_3);
        self.render_gpu_mem_gauge(
            self.gpu_mem_used,
            self.gpu_mem_max,
            frame,
            bottom_left_4,
        );

        // enclosing border for bottom left gauges
        let b = Block::bordered().title(COOLANT_1_LABEL);
        frame.render_widget(b, bottom_left);
    }

    fn render_coolant_gauge(
        &self,
        val: f64,
        block: Block,
        frame: &mut Frame,
        area: Rect,
    ) {
        let label = Span::styled(
            format!("{:.1}C", val),
            Style::new().bold().fg(Color::Gray).bg(Color::Black),
        );

        let color = if val < 34.0 {
            Color::Green
        } else if val < 38.0 {
            Color::Yellow
        } else {
            Color::Red
        };

        let g1 = Gauge::default()
            .block(block)
            .gauge_style(color)
            .ratio(((val - 25.0) / 20.0).clamp(0.0, 1.0))
            .label(label);

        frame.render_widget(g1, area);
    }

    fn render_gpu_watts_gauge(&self, val: f64, frame: &mut Frame, area: Rect) {
        let label = Span::styled(
            format!("{:.1}W / 200W", val),
            Style::new().bold().fg(Color::Gray).bg(Color::Black),
        );

        let g1 = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .title("RTX 4070 Power"),
            )
            .gauge_style(Color::Blue)
            .ratio((val / 200.0).clamp(0.0, 1.0))
            .label(label);

        frame.render_widget(g1, area);
    }

    fn render_gpu_mem_gauge(
        &self,
        used: u64,
        total: u64,
        frame: &mut Frame,
        area: Rect,
    ) {
        let label = Span::styled(
            format!(
                "{}MiB / {}MiB",
                used.to_formatted_string(&Locale::en),
                total.to_formatted_string(&Locale::en)
            ),
            Style::new().bold().fg(Color::Gray).bg(Color::Black),
        );

        let g1 = Gauge::default()
            .block(Block::bordered().title("RTX 4070 Memory"))
            .gauge_style(Color::Yellow)
            .ratio((used as f64 / total as f64).clamp(0.0, 1.0))
            .label(label);

        frame.render_widget(g1, area);
    }

    fn render_temps_table(&self, frame: &mut Frame, area: Rect) {
        let ctl1 = format!("{:.1}", self.tctl.last().unwrap().1);
        let ctl2 = format!("{:.1}", self.tctl_mm.0);
        let ctl3 = format!("{:.1}", self.tctl_mm.1);

        let ccd1 = format!("{:.1}", self.tccd1);
        let ccd2 = format!("{:.1}", self.tccd1_mm.0);
        let ccd3 = format!("{:.1}", self.tccd1_mm.1);

        let cool1_1 = format!("{:.1}", self.coolant1.last().unwrap().1);
        let cool1_2 = format!("{:.1}", self.coolant1_mm.0);
        let cool1_3 = format!("{:.1}", self.coolant1_mm.1);

        let cool2_1 = format!("{:.1}", self.coolant2);
        let cool2_2 = format!("{:.1}", self.coolant2_mm.0);
        let cool2_3 = format!("{:.1}", self.coolant2_mm.1);

        let gpu1 = format!("{:.1}", self.gpu_temp.last().unwrap().1);
        let gpu2 = format!("{:.1}", self.gpu_temp_mm.0);
        let gpu3 = format!("{:.1}", self.gpu_temp_mm.1);

        let rows = [
            Row::new(vec![CPU_CTL_LABEL, &ctl1, &ctl2, &ctl3]),
            Row::new(vec![CPU_CCD_LABEL, &ccd1, &ccd2, &ccd3]),
            Row::new(vec![COOLANT_1_LABEL, &cool1_1, &cool1_2, &cool1_3]),
            Row::new(vec![COOLANT_2_LABEL, &cool2_1, &cool2_2, &cool2_3]),
            Row::new(vec![GPU_LABEL, &gpu1, &gpu2, &gpu3]),
        ];

        let widths = [
            Constraint::Fill(1),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
        ];

        let table = Table::new(rows, widths)
            .column_spacing(1)
            .header(
                Row::new(vec!["Sensor", "Curr", "Min", "Max"])
                    .style(Style::new().bold()),
            )
            .block(Block::bordered());

        frame.render_widget(table, area);
    }

    fn render_temps_chart(&self, frame: &mut Frame, area: Rect) {
        let datasets = vec![
            Dataset::default()
                .name(format!(
                    "{CPU_CTL_LABEL} ({:.1})",
                    self.tctl.last().unwrap().1
                ))
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Red))
                .data(&self.tctl),
            Dataset::default()
                .name(format!(
                    "{COOLANT_1_LABEL} ({:.1})",
                    self.coolant1.last().unwrap().1
                ))
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Blue))
                .data(&self.coolant1),
            Dataset::default()
                .name(format!(
                    "{GPU_LABEL} ({:.1})",
                    self.gpu_temp.last().unwrap().1
                ))
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&self.gpu_temp),
        ];

        let x_labels = vec![
            Span::styled(
                "5m ago",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "2m30s ago",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("now", Style::default().add_modifier(Modifier::BOLD)),
        ];

        let y_min = self
            .tctl
            .iter()
            .zip(self.coolant1.iter())
            .zip(self.gpu_temp.iter())
            .map(|v| v.0 .0 .1.min(v.0 .1 .1).min(v.1 .1))
            .filter(|v| *v >= 0.01)
            .min_by(|a, b| {
                if a <= b {
                    return Ordering::Less;
                }
                Ordering::Greater
            })
            .map(|v| (v - BOUNDS_PADDING).max(BOUNDS_MIN))
            .unwrap_or(BOUNDS_MIN);

        let y_max = self
            .tctl
            .iter()
            .zip(self.coolant1.iter())
            .zip(self.gpu_temp.iter())
            .map(|v| v.0 .0 .1.max(v.0 .1 .1).max(v.1 .1))
            .filter(|v| *v >= 0.01)
            .max_by(|a, b| {
                if a <= b {
                    return Ordering::Less;
                }
                Ordering::Greater
            })
            .map(|v| (v + BOUNDS_PADDING).min(BOUNDS_MAX))
            .unwrap_or(BOUNDS_MAX);

        let labels = (0..6).map(|i| {
            let val = y_min + i as f64 * ((y_max - y_min) / 5.0);
            format!("{:.0}", val).bold()
        });

        let chart = Chart::new(datasets)
            // always show the legend (first constraint will always return true)
            .hidden_legend_constraints((
                Constraint::Min(0),
                Constraint::Ratio(1, 4),
            ))
            .legend_position(Some(LegendPosition::TopLeft))
            .block(Block::bordered())
            .x_axis(
                Axis::default()
                    .style(Style::default().fg(Color::Gray))
                    .labels(x_labels)
                    .bounds(self.window),
            )
            .y_axis(
                Axis::default()
                    .style(Style::default().fg(Color::Gray))
                    .labels(labels)
                    .bounds([y_min, y_max]),
            );

        frame.render_widget(chart, area);
    }
}
