use std::time::{Duration, Instant};

use anyhow::Result;
use lm_sensors::{Initializer, LMSensors};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols,
    text::Span,
    widgets::{Axis, Block, Chart, Dataset, Gauge, GraphType, Row, Table},
    DefaultTerminal, Frame,
};

const INTERVAL: u64 = 2000;
const WINDOW_SIZE: u64 = (5 * 60) / (INTERVAL / 1000);

const CPU_CTL_LABEL: &str = "7800 X3D CTL";
const CPU_CCD_LABEL: &str = "7800 X3D CCD";
const COOLANT_1_LABEL: &str = "Coolant 1";
const COOLANT_2_LABEL: &str = "Coolant 2";
const GPU_LABEL: &str = "RTX 4070";

#[derive(Debug)]
struct SensorValues {
    tctl: f64,
    tccd1: f64,
    coolant1: f64,
    coolant2: f64,
    gpu: f64,
}

fn get_sensor_values(sensors: &LMSensors) -> SensorValues {
    let mut tctl: f64 = 0.0;
    let mut tccd1: f64 = 0.0;
    let mut coolant1: f64 = 0.0;
    let mut coolant2: f64 = 0.0;
    let gpu: f64 = 0.0;

    for chip in sensors.chip_iter(None) {
        if let cname @ ("quadro-hid-3-1" | "k10temp-pci-00c3") =
            chip.name().as_deref().unwrap_or("")
        {
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
                            match (cname, fname) {
                                ("quadro-hid-3-1", "temp1") => coolant1 = t,
                                ("quadro-hid-3-1", "temp2") => coolant2 = t,
                                ("k10temp-pci-00c3", "temp1") => tctl = t,
                                ("k10temp-pci-00c3", "temp3") => tccd1 = t,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    SensorValues {
        tctl,
        tccd1,
        coolant1,
        coolant2,
        gpu,
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
    tctl: Vec<(f64, f64)>,
    tctl_mm: (f64, f64),
    tccd1: Vec<(f64, f64)>,
    tccd1_mm: (f64, f64),
    coolant1: Vec<(f64, f64)>,
    coolant1_mm: (f64, f64),
    coolant2: Vec<(f64, f64)>,
    coolant2_mm: (f64, f64),
    gpu: Vec<(f64, f64)>,
    gpu_mm: (f64, f64),
    window: [f64; 2],
}

impl App {
    fn new() -> Self {
        let sensors: LMSensors = Initializer::default()
            .initialize()
            .expect("Failed to init lm-sensors");

        let mut tctl = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut tccd1 = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut coolant1 = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut coolant2 = Vec::with_capacity(WINDOW_SIZE as usize);
        let mut gpu = Vec::with_capacity(WINDOW_SIZE as usize);

        for i in 0..(WINDOW_SIZE - 1) {
            tctl.push((i as f64, 0.0));
            tccd1.push((i as f64, 0.0));
            coolant1.push((i as f64, 0.0));
            coolant2.push((i as f64, 0.0));
            gpu.push((i as f64, 0.0));
        }

        let values = get_sensor_values(&sensors);
        tctl.push(((WINDOW_SIZE - 1) as f64, values.tctl));
        tccd1.push(((WINDOW_SIZE - 1) as f64, values.tccd1));
        coolant1.push(((WINDOW_SIZE - 1) as f64, values.coolant1));
        coolant2.push(((WINDOW_SIZE - 1) as f64, values.coolant2));
        gpu.push(((WINDOW_SIZE - 1) as f64, values.gpu));

        Self {
            sensors,
            tctl,
            tctl_mm: (values.tctl, values.tctl),
            tccd1,
            tccd1_mm: (values.tccd1, values.tccd1),
            coolant1,
            coolant1_mm: (values.coolant1, values.coolant1),
            coolant2,
            coolant2_mm: (values.coolant2, values.coolant2),
            gpu,
            gpu_mm: (values.gpu, values.gpu),
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
        let vals = get_sensor_values(&self.sensors);

        self.window[0] += 1.0;
        self.window[1] += 1.0;

        let w = self.window[1];

        self.tctl.remove(0);
        self.tccd1.remove(0);
        self.coolant1.remove(0);
        self.coolant2.remove(0);
        self.gpu.remove(0);

        self.tctl.push((w, vals.tctl));
        self.tccd1.push((w, vals.tccd1));
        self.coolant1.push((w, vals.coolant1));
        self.coolant2.push((w, vals.coolant2));
        self.gpu.push((w, vals.gpu));

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
        if vals.gpu < self.gpu_mm.0 {
            self.gpu_mm.0 = vals.gpu
        }
        if vals.gpu > self.gpu_mm.1 {
            self.gpu_mm.1 = vals.gpu
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let [top, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(8)])
                .areas(frame.area());

        let [bottom_left, bottom_right] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(34)])
                .areas(bottom);

        let [bottom_left_top, bottom_left_bottom] = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .areas(bottom_left);

        self.render_temps_chart(frame, top);
        self.render_temps_table(frame, bottom_right);

        let c1 = self.coolant1.last().unwrap().1;
        let b1 = Block::bordered().title("Coolant 1");
        let c2 = self.coolant1.last().unwrap().1;
        let b2 = Block::bordered().title("Coolant 2");
        self.render_coolant_guage(c1, b1, frame, bottom_left_top);
        self.render_coolant_guage(c2, b2, frame, bottom_left_bottom);
    }

    fn render_coolant_guage(
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

    fn render_temps_table(&self, frame: &mut Frame, area: Rect) {
        let ctl1 = format!("{:.1}", self.tctl.last().unwrap().1);
        let ctl2 = format!("{:.1}", self.tctl_mm.0);
        let ctl3 = format!("{:.1}", self.tctl_mm.1);

        let ccd1 = format!("{:.1}", self.tccd1.last().unwrap().1);
        let ccd2 = format!("{:.1}", self.tccd1_mm.0);
        let ccd3 = format!("{:.1}", self.tccd1_mm.1);

        let cool1_1 = format!("{:.1}", self.coolant1.last().unwrap().1);
        let cool1_2 = format!("{:.1}", self.coolant1_mm.0);
        let cool1_3 = format!("{:.1}", self.coolant1_mm.1);

        let cool2_1 = format!("{:.1}", self.coolant2.last().unwrap().1);
        let cool2_2 = format!("{:.1}", self.coolant2_mm.0);
        let cool2_3 = format!("{:.1}", self.coolant2_mm.1);

        let gpu1 = format!("{:.1}", self.gpu.last().unwrap().1);
        let gpu2 = format!("{:.1}", self.gpu_mm.0);
        let gpu3 = format!("{:.1}", self.gpu_mm.1);

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
                    self.gpu.last().unwrap().1
                ))
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&self.gpu),
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

        let chart = Chart::new(datasets)
            // always show the legend (first constraint will always return true)
            .hidden_legend_constraints((
                Constraint::Min(0),
                Constraint::Ratio(1, 4),
            ))
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
                    .labels([
                        "25".bold(),
                        "35".bold(),
                        "45".bold(),
                        "55".bold(),
                        "65".bold(),
                        "75".bold(),
                        "85".bold(),
                    ])
                    .bounds([25.0, 85.0]),
            );

        frame.render_widget(chart, area);
    }
}
