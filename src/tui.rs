use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::{
    algorithm::{AlgorithmEvent, Item, MobilizationSort},
    anthem::{Sequencer, ANTHEM_BPM},
    audio::AudioEngine,
    config::{DEFAULT_COUNT, DEFAULT_MAX, DEFAULT_MIN, DEFAULT_VOLUME, FRAME_MILLIS, MAX_ELEMENTS},
    input,
    visualizer::draw_lane,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Menu,
    Sorting,
    Summary,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormKind {
    Random,
    Manual,
    File,
}

struct Field {
    label: &'static str,
    value: String,
}

struct FormState {
    kind: FormKind,
    fields: Vec<Field>,
    active: usize,
    error: Option<String>,
}

impl FormState {
    fn new(kind: FormKind) -> Self {
        let fields = match kind {
            FormKind::Random => vec![
                Field {
                    label: "Количество (0..3000)",
                    value: DEFAULT_COUNT.to_string(),
                },
                Field {
                    label: "Минимум",
                    value: DEFAULT_MIN.to_string(),
                },
                Field {
                    label: "Максимум",
                    value: DEFAULT_MAX.to_string(),
                },
            ],
            FormKind::Manual => vec![Field {
                label: "Числа (пробелы, запятые или точка с запятой)",
                value: String::new(),
            }],
            FormKind::File => vec![Field {
                label: "Путь к TXT/CSV",
                value: String::new(),
            }],
        };
        Self {
            kind,
            fields,
            active: 0,
            error: None,
        }
    }
}

struct App {
    screen: Screen,
    menu_selection: usize,
    form: Option<FormState>,
    algorithm: Option<MobilizationSort>,
    original_values: Vec<i64>,
    paused: bool,
    muted: bool,
    volume: u8,
    speed: usize,
    scroll: usize,
    highlighted: Option<usize>,
    highlight_until: Option<Instant>,
    music_wait_until: Option<Instant>,
    paused_at: Option<Instant>,
    sequencer: Sequencer,
    status: String,
    audio_error: Option<String>,
    started_at: Option<Instant>,
    elapsed: Duration,
}

impl App {
    fn new(audio_error: Option<String>) -> Self {
        Self {
            screen: Screen::Menu,
            menu_selection: 0,
            form: None,
            algorithm: None,
            original_values: Vec::new(),
            paused: false,
            muted: audio_error.is_some(),
            volume: DEFAULT_VOLUME,
            speed: 1,
            scroll: 0,
            highlighted: None,
            highlight_until: None,
            music_wait_until: None,
            paused_at: None,
            sequencer: Sequencer::default(),
            status: "Выберите источник массива".to_string(),
            audio_error,
            started_at: None,
            elapsed: Duration::ZERO,
        }
    }

    fn on_key(&mut self, key: KeyEvent, audio: Option<&AudioEngine>) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.form.is_none()
            && (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q'))
        {
            self.screen = Screen::Exit;
            return;
        }

        if self.form.is_some() {
            self.handle_form_key(key);
            return;
        }

        match self.screen {
            Screen::Menu => self.handle_menu_key(key, audio),
            Screen::Sorting => self.handle_sort_key(key, audio),
            Screen::Summary => self.handle_summary_key(key, audio),
            Screen::Exit => {}
        }
    }

    fn handle_menu_key(&mut self, key: KeyEvent, audio: Option<&AudioEngine>) {
        match key.code {
            KeyCode::Up => self.menu_selection = self.menu_selection.saturating_sub(1),
            KeyCode::Down => self.menu_selection = (self.menu_selection + 1).min(2),
            KeyCode::Enter => {
                let kind = match self.menu_selection {
                    0 => FormKind::Random,
                    1 => FormKind::Manual,
                    _ => FormKind::File,
                };
                self.form = Some(FormState::new(kind));
                self.status.clear();
            }
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_mute(audio),
            _ => {}
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                self.status = "Выберите источник массива".to_string();
            }
            KeyCode::Tab => {
                if let Some(form) = self.form.as_mut() {
                    if !form.fields.is_empty() {
                        form.active = (form.active + 1) % form.fields.len();
                    }
                    form.error = None;
                }
            }
            KeyCode::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    if let Some(field) = form.fields.get_mut(form.active) {
                        field.value.pop();
                        form.error = None;
                    }
                }
            }
            KeyCode::Enter => {
                let advance_field = self.form.as_ref().is_some_and(|form| {
                    form.kind == FormKind::Random && form.active + 1 < form.fields.len()
                });
                if advance_field {
                    if let Some(form) = self.form.as_mut() {
                        form.active += 1;
                        form.error = None;
                    }
                } else {
                    self.submit_form();
                }
            }
            KeyCode::Char(character) => {
                if !character.is_control() {
                    if let Some(form) = self.form.as_mut() {
                        if let Some(field) = form.fields.get_mut(form.active) {
                            if field.value.chars().count() < 4_096 {
                                field.value.push(character);
                                form.error = None;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn submit_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let result: Result<Vec<i64>, String> = (|| match form.kind {
            FormKind::Random => {
                let count = parse_field::<usize>(form.fields.first(), "Количество")?;
                let min = parse_field::<i64>(form.fields.get(1), "Минимум")?;
                let max = parse_field::<i64>(form.fields.get(2), "Максимум")?;
                input::random_values(count, min, max).map_err(|error| error.to_string())
            }
            FormKind::Manual => input::parse_values(
                form.fields.first().map_or("", |field| field.value.as_str()),
            )
            .map_err(|error| error.to_string()),
            FormKind::File => {
                let path = form
                    .fields
                    .first()
                    .map_or("", |field| field.value.trim());
                if path.is_empty() {
                    Err("Укажите путь к файлу".to_string())
                } else {
                    input::load_file(path.trim_matches('"')).map_err(|error| error.to_string())
                }
            }
        })();

        match result {
            Ok(values) => self.start(values),
            Err(error) => {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(error);
                }
            }
        }
    }

    fn start(&mut self, values: Vec<i64>) {
        self.original_values = values.clone();
        self.algorithm = Some(MobilizationSort::new(values));
        self.form = None;
        self.screen = Screen::Sorting;
        self.paused = false;
        self.scroll = 0;
        self.highlighted = None;
        self.highlight_until = None;
        self.music_wait_until = None;
        self.paused_at = None;
        self.elapsed = Duration::ZERO;
        self.started_at = Some(Instant::now());
        self.sequencer.reset();
        self.status = "Началась мобилизация".to_string();
    }

    fn handle_sort_key(&mut self, key: KeyEvent, audio: Option<&AudioEngine>) {
        match key.code {
            KeyCode::Esc => self.return_to_menu(audio),
            KeyCode::Char(' ') => {
                self.paused = !self.paused;
                if let Some(audio) = audio {
                    audio.set_paused(self.paused);
                }
                if self.paused {
                    self.paused_at = Some(Instant::now());
                } else if let Some(paused_at) = self.paused_at.take() {
                    let paused_for = paused_at.elapsed();
                    if let Some(deadline) = self.music_wait_until.as_mut() {
                        *deadline = *deadline + paused_for;
                    }
                    if let Some(deadline) = self.highlight_until.as_mut() {
                        *deadline = *deadline + paused_for;
                    }
                }
                self.status = if self.paused {
                    "Пауза".to_string()
                } else {
                    "Мобилизация продолжается".to_string()
                };
            }
            KeyCode::Char('r') | KeyCode::Char('R') => self.restart(audio),
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_mute(audio),
            KeyCode::Char('[') => self.change_volume(-5, audio),
            KeyCode::Char(']') => self.change_volume(5, audio),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.speed = (self.speed + 1).min(20);
                self.status = format!("Скорость анимации: x{}", self.speed);
            }
            KeyCode::Char('-') => {
                self.speed = self.speed.saturating_sub(1).max(1);
                self.status = format!("Скорость анимации: x{}", self.speed);
            }
            KeyCode::Left => self.scroll = self.scroll.saturating_sub(3),
            KeyCode::Right => self.scroll = self.scroll.saturating_add(3),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(12),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(12),
            _ => {}
        }
    }

    fn handle_summary_key(&mut self, key: KeyEvent, audio: Option<&AudioEngine>) {
        match key.code {
            KeyCode::Esc => self.return_to_menu(audio),
            KeyCode::Char('r') | KeyCode::Char('R') => self.restart(audio),
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_mute(audio),
            KeyCode::Char('[') => self.change_volume(-5, audio),
            KeyCode::Char(']') => self.change_volume(5, audio),
            KeyCode::Left => self.scroll = self.scroll.saturating_sub(3),
            KeyCode::Right => self.scroll = self.scroll.saturating_add(3),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(12),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(12),
            _ => {}
        }
    }

    fn restart(&mut self, audio: Option<&AudioEngine>) {
        self.algorithm = Some(MobilizationSort::new(self.original_values.clone()));
        self.screen = Screen::Sorting;
        self.paused = false;
        self.scroll = 0;
        self.highlighted = None;
        self.highlight_until = None;
        self.music_wait_until = None;
        self.paused_at = None;
        self.elapsed = Duration::ZERO;
        self.started_at = Some(Instant::now());
        self.sequencer.reset();
        self.status = "Новая мобилизация".to_string();
        if let Some(audio) = audio {
            audio.stop();
            audio.set_paused(false);
        }
    }

    fn return_to_menu(&mut self, audio: Option<&AudioEngine>) {
        self.screen = Screen::Menu;
        self.algorithm = None;
        self.form = None;
        self.paused = false;
        self.highlighted = None;
        self.highlight_until = None;
        self.music_wait_until = None;
        self.paused_at = None;
        if let Some(audio) = audio {
            audio.stop();
            audio.set_paused(false);
        }
        self.status = "Выберите источник массива".to_string();
    }

    fn toggle_mute(&mut self, audio: Option<&AudioEngine>) {
        let Some(audio) = audio else {
            self.muted = true;
            self.music_wait_until = None;
            self.status = self
                .audio_error
                .as_ref()
                .map_or_else(|| "Аудиоустройство недоступно".to_string(), |error| {
                    format!("Аудио недоступно: {error}")
                });
            return;
        };

        self.muted = !self.muted;
        audio.set_muted(self.muted);
        if self.muted {
            audio.stop();
            self.music_wait_until = None;
        }
        self.status = if self.muted {
            "Музыка отключена".to_string()
        } else {
            "Музыка включена".to_string()
        };
    }

    fn change_volume(&mut self, delta: i16, audio: Option<&AudioEngine>) {
        self.volume = (i16::from(self.volume) + delta).clamp(0, 100) as u8;
        if let Some(audio) = audio {
            audio.set_volume(self.volume);
        }
        self.status = format!("Громкость: {}%", self.volume);
    }

    fn advance(&mut self, audio: Option<&AudioEngine>) {
        if self.screen != Screen::Sorting || self.paused {
            return;
        }
        if self
            .music_wait_until
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return;
        }
        self.music_wait_until = None;

        let Some(n) = self.algorithm.as_ref().map(MobilizationSort::total) else {
            return;
        };
        let steps = ((1 + n / 80) * self.speed).clamp(1, 96);
        for _ in 0..steps {
            let Some(event) = self.algorithm.as_mut().map(MobilizationSort::step) else {
                break;
            };
            match event {
                AlgorithmEvent::WaveStarted {
                    wave,
                    percent,
                    target,
                    recruited,
                } => {
                    self.status = format!(
                        "Волна {wave}: охват {percent}% → в формации {target}, набор {recruited}"
                    );
                }
                AlgorithmEvent::Recruited { wave, item } => {
                    self.highlight_movement(item.id);
                    self.status = format!("Волна {wave}: элемент {} отправлен в отряд", item.value);
                    let note_duration = self.play_recruitment_note(audio);
                    if !note_duration.is_zero() {
                        self.music_wait_until = Some(Instant::now() + note_duration);
                    }
                    // One recruitment, one note, one visible red transition.
                    break;
                }
                AlgorithmEvent::BatchMove { wave, item } => {
                    self.highlight_movement(item.id);
                    self.status = format!("Волна {wave}: сортировка мобилизованного отряда");
                }
                AlgorithmEvent::BatchSorted { wave, count } => {
                    self.status = format!("Волна {wave}: отсортирован отряд из {count} элементов");
                }
                AlgorithmEvent::Merged { wave, item, .. } => {
                    self.highlight_movement(item.id);
                    self.status = format!("Волна {wave}: элемент {} возвращён в строй", item.value);
                }
                AlgorithmEvent::WaveCompleted { wave, sorted, total } => {
                    self.clear_highlight();
                    self.status = format!("Волна {wave} завершена: в отсортированной формации {sorted}/{total}");
                }
                AlgorithmEvent::InvariantFailure { wave } => {
                    self.clear_highlight();
                    self.elapsed = self
                        .started_at
                        .map_or(Duration::ZERO, |started| started.elapsed());
                    self.screen = Screen::Summary;
                    self.status = format!("Проверка инварианта волны {wave} не пройдена");
                    break;
                }
                AlgorithmEvent::Finished => {
                    self.clear_highlight();
                    self.elapsed = self
                        .started_at
                        .map_or(Duration::ZERO, |started| started.elapsed());
                    self.screen = Screen::Summary;
                    self.status = if self.algorithm.as_ref().is_some_and(MobilizationSort::is_sorted) {
                        "Массив действительно отсортирован".to_string()
                    } else {
                        "Алгоритм завершился с ошибкой сортировки".to_string()
                    };
                    break;
                }
            }
        }
    }

    fn play_recruitment_note(&mut self, audio: Option<&AudioEngine>) -> Duration {
        let note = self.sequencer.next();
        if self.muted {
            return Duration::ZERO;
        }
        let Some(audio) = audio else {
            return Duration::ZERO;
        };

        let sounding_ms = audio.play(note, ANTHEM_BPM);
        Duration::from_millis(sounding_ms.saturating_add(note.rest_ms(ANTHEM_BPM)))
    }

    fn highlight_movement(&mut self, item_id: usize) {
        self.highlighted = Some(item_id);
        self.highlight_until = Some(Instant::now() + Duration::from_millis(100));
    }

    fn clear_highlight(&mut self) {
        self.highlighted = None;
        self.highlight_until = None;
    }

    fn active_highlight(&self) -> Option<usize> {
        self.highlight_until
            .filter(|deadline| Instant::now() < *deadline)
            .and(self.highlighted)
    }

}

fn parse_field<T: std::str::FromStr>(field: Option<&Field>, label: &str) -> Result<T, String> {
    let text = field.map_or("", |field| field.value.trim());
    text.parse::<T>()
        .map_err(|_| format!("Некорректное значение поля «{label}»"))
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run() -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let (audio, audio_error) = match AudioEngine::new() {
        Ok(engine) => (Some(engine), None),
        Err(error) => (None, Some(error)),
    };
    if let Some(audio) = &audio {
        audio.set_volume(DEFAULT_VOLUME);
    }
    let mut app = App::new(audio_error);

    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if app.screen == Screen::Exit {
            break;
        }

        if event::poll(Duration::from_millis(FRAME_MILLIS))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key, audio.as_ref()),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        app.advance(audio.as_ref());
    }
    Ok(())
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 36 || area.height < 18 {
        frame.render_widget(
            Paragraph::new("Увеличьте окно терминала (минимум 36×18)"),
            area,
        );
        return;
    }

    match app.screen {
        Screen::Menu => draw_menu(frame, area, app),
        Screen::Sorting => draw_sorting(frame, area, app),
        Screen::Summary => draw_summary(frame, area, app),
        Screen::Exit => {}
    }
}

fn draw_menu(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(7), Constraint::Length(3)])
        .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                " MOBILIZATION SORT",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(" Шуточная TUI-сортировка с мобилизационными волнами и гимном"),
        ])
        .block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    if let Some(form) = &app.form {
        draw_form(frame, chunks[1], form);
    } else {
        let options = [
            "Random  — случайные числа (по умолчанию 50, диапазон 1…100)",
            "Manual  — ввести числа вручную",
            "File    — загрузить TXT/CSV",
        ];
        let mut lines = vec![Line::from("")];
        for (index, option) in options.iter().enumerate() {
            let marker = if app.menu_selection == index { "▶ " } else { "  " };
            let style = if app.menu_selection == index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(format!("{marker}{option}"), style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(
            "Волны: накопленное покрытие 10% → 25% → 50% → 75% → 100%.",
        ));
        lines.push(Line::from(
            "Отрицательные и повторяющиеся значения поддерживаются.",
        ));
        if let Some(error) = &app.audio_error {
            lines.push(Line::from(Span::styled(
                format!("Аудио недоступно: {error}"),
                Style::default().fg(Color::Red),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().title("Источник массива").borders(Borders::ALL))
                .wrap(Wrap { trim: true }),
            chunks[1],
        );
    }

    frame.render_widget(
        Paragraph::new(format!(
            "↑/↓ выбор • Enter подтвердить • M музыка {} • Q выход",
            if app.muted { "выкл." } else { "вкл." }
        ))
        .block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

fn draw_form(frame: &mut Frame<'_>, area: Rect, form: &FormState) {
    let title = match form.kind {
        FormKind::Random => "Настройка случайного массива",
        FormKind::Manual => "Ручной ввод",
        FormKind::File => "Загрузка файла",
    };
    let mut lines = vec![Line::from("")];
    for (index, field) in form.fields.iter().enumerate() {
        let marker = if index == form.active { "▶" } else { " " };
        let value = if field.value.is_empty() {
            "_".to_string()
        } else {
            field.value.clone()
        };
        let style = if index == form.active {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {}: {value}", field.label),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Enter — далее/запуск • Tab — следующее поле • Esc — назад"));
    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("Ошибка: {error}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_sorting(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    let Some(algorithm) = app.algorithm.as_ref() else {
        return;
    };
    let stats = algorithm.stats();
    let active_highlight = app.active_highlight();
    let header = format!(
        "Mobilization Sort  |  собрано: {}/{}  |  осталось в резерве: {}  |  волн: {}  |  скорость x{}",
        algorithm.formation_view().len(),
        algorithm.total(),
        algorithm.remaining(),
        stats.waves,
        app.speed
    );
    frame.render_widget(
        Paragraph::new(header)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    let lanes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(33), Constraint::Percentage(33)])
        .split(chunks[1]);
    let (min_value, max_value) = value_range(&app.original_values);
    let formation: Vec<Option<Item>> = algorithm
        .formation_view()
        .iter()
        .copied()
        .map(Some)
        .collect();
    let batch = algorithm.batch_view();
    draw_lane(
        frame,
        lanes[0],
        "ОТСОРТИРОВАННАЯ ФОРМАЦИЯ",
        &formation,
        Color::Green,
        active_highlight,
        min_value,
        max_value,
        app.scroll,
    );
    draw_lane(
        frame,
        lanes[1],
        "МОБИЛИЗОВАННЫЙ ОТРЯД",
        &batch,
        Color::Yellow,
        active_highlight,
        min_value,
        max_value,
        app.scroll,
    );
    draw_lane(
        frame,
        lanes[2],
        "РЕЗЕРВ",
        algorithm.reserve_view(),
        Color::Cyan,
        active_highlight,
        min_value,
        max_value,
        app.scroll,
    );

    let footer = format!(
        "{}  |  Space пауза  R заново  M музыка {}  [/] громкость {}%  +/- скорость  ←/→ прокрутка  Esc меню  Q выход",
        app.status,
        if app.muted { "выкл." } else { "вкл." },
        app.volume
    );
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(if app.paused { Color::Yellow } else { Color::White }))
            .block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

fn draw_summary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(5), Constraint::Length(3)])
        .split(area);
    let Some(algorithm) = app.algorithm.as_ref() else {
        return;
    };
    let stats = algorithm.stats();
    let result = if algorithm.is_sorted() {
        "ГОТОВО — массив отсортирован по возрастанию"
    } else {
        "ОШИБКА — проверка результата не пройдена"
    };
    let summary = vec![
        Line::from(Span::styled(
            result,
            Style::default()
                .fg(if algorithm.is_sorted() { Color::Green } else { Color::Red })
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "Элементов: {}  •  волн: {}  •  проверок волн: {}  •  изъято: {}  •  перемещений: {}  •  сравнений: {}  •  время: {:.2} с",
            algorithm.total(),
            stats.waves,
            stats.wave_checks,
            stats.elements_recruited,
            stats.sort_moves + stats.merge_moves,
            stats.comparisons,
            app.elapsed.as_secs_f64()
        )),
        Line::from("Зелёная формация — фактически собранный отсортированный результат."),
    ];
    frame.render_widget(
        Paragraph::new(summary)
            .block(Block::default().title("Итоги мобилизации").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        chunks[0],
    );

    let values: Vec<Option<Item>> = algorithm
        .formation_view()
        .iter()
        .copied()
        .map(Some)
        .collect();
    let (min_value, max_value) = value_range(&app.original_values);
    draw_lane(
        frame,
        chunks[1],
        "ОТСОРТИРОВАННЫЙ МАССИВ",
        &values,
        Color::Green,
        None,
        min_value,
        max_value,
        app.scroll,
    );
    frame.render_widget(
        Paragraph::new("R повтор • Esc меню • M музыка • ←/→ прокрутка • Q выход")
            .block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

fn value_range(values: &[i64]) -> (i64, i64) {
    values.iter().copied().fold((0, 0), |(min, max), value| {
        (min.min(value), max.max(value))
    })
}


#[cfg(test)]
mod tests {
    use super::{App, Duration, Instant};

    #[test]
    fn anthem_sequence_advances_only_on_recruitment() {
        let mut app = App::new(None);
        app.start(vec![42]);

        app.advance(None); // WaveStarted
        assert_eq!(app.sequencer.position(), 0);
        app.advance(None); // Recruited
        assert_eq!(app.sequencer.position(), 1);

        for _ in 0..4 {
            app.advance(None); // BatchSorted, Merged, WaveCompleted, Finished
            assert_eq!(app.sequencer.position(), 1);
        }
    }

    #[test]
    fn movement_highlight_expires() {
        let mut app = App::new(None);
        app.highlighted = Some(7);
        app.highlight_until = Some(Instant::now() + Duration::from_secs(60));
        assert_eq!(app.active_highlight(), Some(7));

        app.highlight_until = Some(Instant::now());
        assert_eq!(app.active_highlight(), None);
    }
}
