use crate::island::*;
use crate::platform::{localvoice, sounds, speech, wake, wake_install};
use crate::services::assistant::agent::{self, Phase, Run};
use crate::services::assistant::tools::{self, Tool};
use crate::services::assistant::{Cap, Level, env, memory, recent, shell};
use crate::services::attach::{self, Attachment};
use crate::ui::i18n::tr;
use crate::ui::pet_speech::VoicePhase;

impl App {
    fn voice_phase(&mut self, phase: VoicePhase) {
        self.pet.voice_phase = phase;
        self.pet.char_key = None;
        self.set_timer(Timer::Char, 1);
    }

    fn cue(&self, cue: sounds::Cue) {
        sounds::play(cue, self.settings.assistant.cue_volume());
    }

    fn fail_by_voice(&mut self, text: &str) {
        crate::log_warn!("Assistant failed: {}", brief(text, 300));
        self.cue(sounds::Cue::Error);
        self.notify_for(Notice::Error, '\u{E783}', text, 4000);
    }

    fn clear_notice(&mut self) {
        self.kill_timer(Timer::Notice);
        self.ui.notice = None;
        self.notice_link = None;
        self.update_eq();
        self.repaint_collapsed();
    }

    pub(in crate::island) fn toggle_dictation(&mut self) {
        self.toggle_voice_input(self.settings.pet_voice_input && self.pet.desktop.is_some());
    }

    pub(in crate::island) fn pet_voice_input(&mut self) {
        if !self.settings.pet_voice_input {
            return;
        }
        if self.pet.voice_phase.pending() && !dictation::active()
            || speech::busy()
            || self.agent.as_ref().is_some_and(|run| run.phase == Phase::Speaking)
        {
            self.cancel_voice_input();
        } else {
            self.toggle_voice_input(true);
        }
    }

    pub(in crate::island) fn cancel_voice_input(&mut self) -> bool {
        if self.pet.voice_phase == VoicePhase::Idle && self.agent.is_none() && !speech::busy() {
            return false;
        }
        dictation::stop();
        speech::stop();
        self.pending_power = None;
        self.pending_ask = None;
        self.clear_notice();
        self.end_wake_session();
        true
    }

    fn toggle_voice_input(&mut self, directed: bool) {
        if dictation::active() {
            dictation::stop();
            self.voice_phase(VoicePhase::Transcribing);
            self.cue(sounds::Cue::Done);
            self.show_notice_for('\u{F801}', "Recognizing…", 60_000);
            return;
        }
        if self.pet.voice_phase.pending() || ai::busy() {
            self.notify_for(Notice::Error, '\u{E783}', "Another request is running, try again in a moment", 2500);
            return;
        }
        if !self.speech_ready() {
            return;
        }
        wake::pause();
        speech::stop();
        if dictation::start(self.hwnd) {
            self.wake_session = directed;
            self.voice_phase(VoicePhase::Recording);
            self.cue(sounds::Cue::Heard);
            self.show_notice_for(
                '\u{E720}',
                "Listening… press the keys again",
                (dictation::MAX_SECS as u32 + 5) * 1000,
            );
        } else {
            self.sync_wake();
        }
    }

    pub(in crate::island) fn sync_wake(&mut self) {
        let a = &self.settings.assistant;
        let want = a.perms.enabled && a.wake && wake::installed();
        if !want {
            if wake::listening() {
                wake::stop();
            }
            return;
        }
        if !wake::listening() {
            wake::start(self.hwnd, voicecmd::wake_words(&self.settings.ai.voice_wake));
        }
        let speaking = self.agent.as_ref().is_some_and(|r| r.phase == Phase::Speaking);
        if (self.wake_session && !speaking) || dictation::active() {
            wake::pause();
        } else {
            wake::resume();
        }
    }

    pub(in crate::island) fn restart_wake(&mut self) {
        if wake::listening() {
            wake::stop();
            self.sync_wake();
        }
    }

    pub(in crate::island) fn end_wake_session(&mut self) {
        self.voice_phase(VoicePhase::Idle);
        self.wake_session = false;
        if let Some(run) = self.agent.take() {
            self.keep_run(&run);
        }
        self.sync_wake();
    }

    fn keep_run(&mut self, run: &Run) {
        if let Some(exchange) = run.exchange() {
            self.dialog.push(exchange);
            agent::fresh(&mut self.dialog);
        }
        let remembers = self.settings.assistant.perms.command_level(Cap::Memory, false) != Level::Off;
        if remembers && (!run.did.is_empty() || !run.spoken.is_empty()) {
            let when = chrono::Local::now().format("%H:%M").to_string();
            recent::record(&recent::entry(&when, &run.request, &run.did, &run.spoken));
        }
    }

    fn voice_choice(&self) -> speech::Voice {
        let a = &self.settings.assistant;
        speech::Voice {
            fish_streaming: a.fish_streaming,
            fish_expressions: a.fish_expressions,
            local: a.local_voice && localvoice::ready(),
            natural: a.natural_voice.then(|| speech::Natural {
                provider: a.tts_provider,
                base: a.tts_base.clone(),
                model: a.tts_model.clone(),
                voice: a.tts_voice.clone(),
            }),
        }
    }

    pub(in crate::island) fn install_local_voice(&mut self) {
        if localvoice::install(self.hwnd) {
            self.show_notice_for('\u{E896}', "Downloading the local voice…", 120_000);
        } else {
            self.notify_for(Notice::Error, '\u{E783}', "Another voice model is being downloaded", 3000);
        }
    }

    pub(in crate::island) fn on_local_voice_setup(&mut self) {
        if self.refresh_guide_voice() {
            return self.render_settings_win();
        }
        self.clear_notice();
        match localvoice::state() {
            localvoice::State::Ready => {
                self.cue(sounds::Cue::Done);
                self.show_notice_for('\u{E73E}', "The local voice is installed: turn it on in the settings", 3500);
            }
            localvoice::State::Failed(why) => self.fail_by_voice(why),
            _ => {}
        }
    }

    fn quiet_or_fail(&mut self, why: &str) {
        if self.agent.as_ref().is_some_and(|r| r.following) {
            crate::log_info!("Assistant: the conversation was not continued ({})", brief(why, 80));
        } else {
            self.fail_by_voice(why);
        }
    }

    pub(in crate::island) fn install_wake_runtime(&mut self) {
        if wake_install::start(self.hwnd) {
            self.show_notice_for('\u{E896}', "Downloading the wake-word model…", 120_000);
        } else {
            self.notify_for(Notice::Error, '\u{E783}', "Another voice model is being downloaded", 3000);
        }
    }

    pub(in crate::island) fn on_wake_setup(&mut self) {
        if self.refresh_guide_voice() {
            self.sync_wake();
            return self.render_settings_win();
        }
        self.clear_notice();
        match wake_install::state() {
            wake_install::State::Ready => {
                self.cue(sounds::Cue::Done);
                self.show_notice_for('\u{E73E}', "Wake-word model installed", 2500);
                self.sync_wake();
            }
            wake_install::State::Failed(why) => self.fail_by_voice(why),
            _ => {}
        }
    }

    fn speech_ready(&mut self) -> bool {
        if !ai::can_transcribe(&self.settings.ai) {
            self.notify_for(
                Notice::Error,
                '\u{E783}',
                "Dictation needs Gemini, an OpenAI-compatible service or a separate speech service",
                4000,
            );
            return false;
        }
        if !ai::voice_ready(&self.settings.ai) {
            self.notify_for(Notice::Error, '\u{E783}', "Save a key for the speech service first", 3500);
            return false;
        }
        true
    }

    pub(in crate::island) fn on_wake(&mut self) {
        if !self.settings.assistant.perms.enabled || !self.speech_ready() {
            return self.sync_wake();
        }
        speech::stop();
        if self.settings.assistant.local_voice {
            localvoice::warm_up();
        }
        if self.agent.as_ref().is_some_and(|r| r.phase == Phase::Working) {
            crate::log_info!("Assistant: the call word cancelled the request that was still running");
            self.agent = None;
        }
        let interrupted = self.agent.as_ref().is_some_and(|r| r.phase == Phase::Speaking);
        if interrupted && let Some(run) = self.agent.as_mut() {
            crate::log_info!("Assistant: interrupted by the call word while speaking");
            run.phase = Phase::Listening;
            run.waiting = true;
        }
        self.wake_session = true;
        if dictation::start_auto(self.hwnd) {
            self.voice_phase(VoicePhase::Recording);
            self.cue(sounds::Cue::Heard);
            self.show_notice_for('\u{E720}', "Listening…", (dictation::MAX_SECS as u32 + 5) * 1000);
        } else {
            self.end_wake_session();
        }
    }

    pub(in crate::island) fn on_voice(&mut self) {
        let recording = self.pet.voice_phase != VoicePhase::Idle;
        let outcome = dictation::take();
        if !recording {
            self.sync_wake();
            return;
        }
        self.voice_phase(VoicePhase::Idle);
        match outcome {
            Some(Ok(wav)) => {
                if self.wake_session {
                    self.cue(sounds::Cue::Done);
                }
                if ai::transcribe(self.hwnd, wav) {
                    self.voice_phase(VoicePhase::Transcribing);
                    self.show_notice_for('\u{F801}', "Recognizing…", 60_000);
                } else {
                    self.clear_notice();
                    self.fail_by_voice("Another request is running, try again in a moment");
                    self.end_wake_session();
                }
            }
            Some(Err(why)) => {
                self.clear_notice();
                self.quiet_or_fail(why);
                self.end_wake_session();
            }
            None => {
                self.clear_notice();
                self.end_wake_session();
            }
        }
    }

    fn start_voice_command(&mut self, command: &str) {
        if command.is_empty() {
            self.show_notice_for('\u{E720}', "Say a command after the name", 2200);
            return self.end_wake_session();
        }
        let request: String = command.chars().take(600).collect();
        crate::log_info!("Assistant request: {}", brief(&request, 300));
        let perms = &self.settings.assistant.perms;
        if self.settings.assistant.yandex_voice
            && let Some(action) = voicecmd::local_yandex(&request)
        {
            crate::log_info!("Assistant action from speech: Yandex({action:?})");
            self.run_serial += 1;
            let mut run = Run::new(String::new(), &request, &[]);
            run.id = self.run_serial;
            run.direct = true;
            self.agent = Some(run);
            return self.handle_tool(Tool::Yandex(action), false, false);
        }
        let mut notes = String::new();
        if perms.command_level(Cap::Memory, false) != Level::Off {
            notes = memory::prompt_block(&memory::load(), 1500);
            let earlier = recent::prompt_block(&recent::load(), 900);
            if !earlier.is_empty() {
                notes.push_str(&format!("Earlier requests, oldest first (what happened before):\n{earlier}"));
            }
        }
        let (offered, can_ask) = (perms.commands_offered(), perms.enabled);
        let mut system = voicecmd::system_prompt(&offered, can_ask, &notes, &self.facts());
        if self.settings.assistant.yandex_voice && offered.contains(&Cap::Media) {
            system.push_str(voicecmd::YANDEX_PROMPT);
        }
        if self.settings.assistant.natural_voice
            && self.settings.assistant.fish_expressions
            && self.settings.assistant.tts_provider == crate::services::assistant::TtsProvider::Fish
        {
            system.push_str(&format!(
                "\nFish Audio reads answer and ask aloud. When it suits the user's request, add a short delivery cue to the spoken text only. Allowed cues: {}. Keep the opening sentence short and informative.",
                speech::FISH_CUES.join(" "),
            ));
        }
        agent::fresh(&mut self.dialog);
        let mut run = Run::new(system, &request, &self.dialog);
        run.max_steps = usize::from(self.settings.assistant.max_steps);
        self.run_serial += 1;
        run.id = self.run_serial;
        self.agent = Some(run);
        self.call_agent();
    }

    fn call_agent(&mut self) {
        let Some(run) = self.agent.as_mut() else { return };
        let exhausted = run.out_of_steps();
        if !exhausted {
            run.steps += 1;
            run.waiting = true;
            run.phase = Phase::Working;
        }
        let (turns, system, step, id) = (run.turns_for_call(), run.system.clone(), run.steps, run.id);
        if exhausted {
            self.fail_by_voice("The assistant needed too many steps");
            return self.end_wake_session();
        }
        if step == 1 && self.settings.assistant.local_voice {
            localvoice::warm_up();
        }
        crate::log_info!("Assistant step {step}: asking the model ({} turns)", turns.len());
        if ai::ask_agent(self.hwnd, turns, system, id) {
            self.voice_phase(VoicePhase::Command);
            self.show_notice_for('\u{E99A}', "Thinking…", 60_000);
        } else {
            self.fail_by_voice("Another request is running, try again in a moment");
            self.end_wake_session();
        }
    }

    fn noise_in(&self, heard: &str) -> Option<voicecmd::Heard> {
        let wakes = voicecmd::wake_words(&self.settings.ai.voice_wake);
        let command = voicecmd::strip_wake(heard, &wakes).unwrap_or_else(|| heard.to_string());
        let answering = self.agent.as_ref().is_some_and(|r| r.phase == Phase::Listening && !r.following);
        match voicecmd::judge(&command, &self.last_spoken, answering) {
            voicecmd::Heard::Fine => None,
            noise => Some(noise),
        }
    }

    fn say(&mut self, text: &str) {
        self.last_spoken = speech::plain_text(text);
        let visible_text = self.last_spoken.clone();
        self.notify_for(Notice::Ai, '\u{E8F2}', &visible_text, 8000);
        let (volume, voice) = (self.settings.assistant.voice_volume(), self.voice_choice());
        let chat = self.agent.as_ref().is_some_and(|r| r.did.is_empty() && r.follow_ups < agent::MAX_FOLLOW_UPS);
        let follow = chat && self.settings.assistant.follow_up && self.settings.assistant.perms.enabled;
        if let Some(run) = self.agent.as_mut() {
            run.spoken.clone_from(&self.last_spoken);
            if follow {
                run.waiting = true;
                run.following = true;
                run.phase = Phase::Speaking;
            }
        }
        if follow {
            self.wake_session = true;
            self.sync_wake();
            speech::speak_then_notify(text, volume, self.hwnd, voice);
        } else {
            speech::speak(text, volume, voice);
        }
    }

    fn wait_for_result(&mut self) {
        self.voice_phase(VoicePhase::Command);
        if let Some(run) = self.agent.as_mut() {
            run.waiting = true;
        }
    }

    fn confirm_text(&self, question: &str) -> String {
        if self.settings.assistant.perms.enabled {
            tr(question).into_owned()
        } else {
            format!("{} {}", tr(question), tr("Say the name and \"yes\""))
        }
    }

    fn ask_user(&mut self, glyph: char, text: &str) {
        self.last_spoken = speech::plain_text(text);
        let visible_text = self.last_spoken.clone();
        self.notify_for(Notice::Integrations, glyph, &visible_text, (voicecmd::CONFIRM_SECS * 1000) as u32);
        if !self.settings.assistant.perms.enabled || self.agent.is_none() {
            return;
        }
        if let Some(run) = self.agent.as_mut() {
            run.waiting = true;
            run.phase = Phase::Speaking;
        }
        self.wake_session = true;
        self.sync_wake();
        let (volume, voice) = (self.settings.assistant.voice_volume(), self.voice_choice());
        speech::speak_then_notify(text, volume, self.hwnd, voice);
    }

    pub(in crate::island) fn on_spoken(&mut self) {
        if !self.agent.as_ref().is_some_and(|r| r.phase == Phase::Speaking) {
            return;
        }
        if !self.speech_ready() {
            return self.end_wake_session();
        }
        if let Some(run) = self.agent.as_mut() {
            run.phase = Phase::Listening;
        }
        if dictation::start_auto(self.hwnd) {
            self.voice_phase(VoicePhase::Recording);
            self.cue(sounds::Cue::Heard);
            self.show_notice_for('\u{E720}', "Listening…", (dictation::MAX_SECS as u32 + 5) * 1000);
        } else {
            self.end_wake_session();
        }
    }

    fn open_by_voice(&mut self, name: &str) -> bool {
        if let Some((title, launch)) = voicecmd::builtin_app(name) {
            cmd::shell_open(launch, None);
            self.show_notice_for('\u{E71D}', title, 2500);
            return true;
        }
        let game = steam::find(name, 1).into_iter().next();
        let app = apps::find(name, 1).into_iter().next();
        let said = name.trim().to_lowercase();
        let game_exact = game.as_ref().is_some_and(|g| g.name.to_lowercase() == said);
        if let Some(g) = game.filter(|_| game_exact || app.is_none()) {
            cmd::shell_open(&steam::launch_url(g.id), None);
            self.show_notice_for('\u{E7FC}', &g.name, 2500);
            return true;
        }
        match app {
            Some(app) => {
                cmd::shell_open(&app.launch, None);
                self.show_notice_for('\u{E71D}', &app.name, 2500);
                true
            }
            None => false,
        }
    }

    fn open_missed(&mut self, name: &str) {
        let can_search =
            self.agent.is_some() && self.settings.assistant.perms.command_level(Cap::Shell, false) != Level::Off;
        if !can_search {
            return self.notify_for(Notice::Error, '\u{E783}', &format!("Nothing called {name} found"), 3500);
        }
        let why = format!("No program or game called {name} in the Start menu or in Steam");
        if let Some(run) = self.agent.as_mut() {
            run.result("open", &Err(why));
        }
        self.call_agent();
    }

    pub(in crate::island) fn on_voice_command(&mut self) {
        let Some((serial, answer)) = ai::take_command_result() else { return };
        if self.agent.as_ref().is_none_or(|run| run.id != serial) {
            crate::log_info!("Assistant: dropped the model's answer to a request that is no longer running");
            return;
        }
        self.voice_phase(VoicePhase::Idle);
        self.clear_notice();
        let reply = match answer {
            Ok(reply) => reply,
            Err(why) => {
                crate::log_warn!("Assistant: the model failed: {}", brief(&why, 300));
                let short: String = why.chars().take(70).collect();
                self.fail_by_voice(&short);
                return self.end_wake_session();
            }
        };
        crate::log_info!("Assistant: the model replied: {}", brief(&reply, 600));
        let commands = voicecmd::parse_many(&reply);
        if let Some(run) = self.agent.as_mut() {
            run.said(&reply);
            run.waiting = false;
            if voicecmd::malformed(&reply, &commands) {
                if run.retries < MAX_RETRIES {
                    run.retries += 1;
                    run.heard(RETRY_HINT);
                    crate::log_warn!("Assistant: the reply was not a valid action, asking again ({})", run.retries);
                    return self.call_agent();
                }
                crate::log_warn!("Assistant: the reply was still not a valid action, giving up");
                self.fail_by_voice("The assistant did not understand the model's reply");
                return self.end_wake_session();
            }
        }
        for command in commands {
            crate::log_info!("Assistant action: {}", brief(&format!("{command:?}"), 400));
            if let (Some(run), Some(label)) = (self.agent.as_mut(), action_label(&command)) {
                run.did.push(label);
            }
            self.handle_command(command, false);
            if self.agent.as_ref().is_none_or(|r| r.waiting) {
                return;
            }
        }
        crate::log_info!("Assistant: the request is done");
        self.end_wake_session();
    }

    fn facts(&self) -> String {
        let now = chrono::Local::now();
        let up = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        let mut extra = String::new();
        let rules = memory::load_instructions(1500);
        if !rules.is_empty() {
            extra.push_str(&format!(
                "The user's own standing instructions (written by the user, follow them):\n{rules}"
            ));
        }
        let wave = crate::services::yandex::snapshot();
        if wave.has_token {
            extra.push_str(&format!(
                "Yandex Music is connected. My Wave: {}.\n",
                if wave.active { "active" } else { "stopped" }
            ));
        }
        let scripts = tools::scripts_prompt(700);
        if !scripts.is_empty() {
            extra.push_str(&format!("Saved scripts (run_script): {scripts}\n"));
        }
        format!(
            "Local time: {}\nThe computer has been on for {} hours {} minutes since it started\nUser folder: {home}\n{}{extra}",
            now.format("%A, %d %B %Y, %H:%M"),
            up / 3600,
            up % 3600 / 60,
            env::facts()
        )
    }

    fn handle_tool(&mut self, tool: Tool, reviewed: bool, confirmed: bool) {
        if matches!(tool, Tool::Yandex(_)) && !self.settings.assistant.yandex_voice {
            self.fail_by_voice("Voice commands for My Wave are disabled");
            return self.end_wake_session();
        }
        let cap = tool.cap();
        let tainted = self.settings.assistant.ask_after_web && self.agent.as_ref().is_some_and(|r| r.tainted);
        let level = self.settings.assistant.perms.command_level(cap, tainted);
        crate::log_info!("Assistant tool {}: level {level:?}, tainted {tainted}, confirmed {confirmed}", tool.name());
        if level == Level::Off {
            let text = format!("{}: {}", tr(cap.title()), tr("off in the Assistant settings"));
            self.fail_by_voice(&text);
            if self.agent.as_ref().is_some_and(|run| run.direct) {
                self.end_wake_session();
            }
            return;
        }
        let asks = level == Level::Ask && !confirmed && !tool.looks_only();
        if asks && tool.script() && self.settings.assistant.review && !reviewed {
            self.wait_for_result();
            shell::review(self.hwnd, tool, self.agent.as_ref().map_or(0, |run| run.id));
            return self.show_notice_for('\u{E9D9}', "Checking the command…", 60_000);
        }
        if asks {
            let detail: String =
                tool.detail().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(100).collect();
            let verb = match tool {
                Tool::Cmd(_) | Tool::PowerShell(_) | Tool::Wsl(_) | Tool::RunScript { .. } => "Run:",
                Tool::SaveScript { .. } => "Save the script:",
                Tool::Spotify { .. } => "Play on Spotify:",
                Tool::Yandex(_) => "My Wave",
                Tool::Fetch(_) | Tool::Render(_) => "Open the page:",
                Tool::Search(_) => "Search the web:",
                Tool::ReadFile(_) => "Read:",
                Tool::WriteFile { .. } => "Write:",
            };
            let text = self.confirm_text(&format!("{} {detail}?", tr(verb)));
            self.pending_ask = Some((voicecmd::Command::Tool(tool), std::time::Instant::now()));
            return self.ask_user('\u{E756}', &text);
        }
        let signature = tool.signature();
        if self.agent.as_ref().is_some_and(|r| r.repeated(&signature)) {
            crate::log_warn!("Assistant: the same {} was asked for twice, not running it again", tool.name());
            if let Some(run) = self.agent.as_mut() {
                run.result(tool.name(), &Err(REPEAT_HINT.to_string()));
            }
            return self.call_agent();
        }
        self.wait_for_result();
        self.show_notice_for('\u{E756}', "Running…", 60_000);
        shell::execute(self.hwnd, tool, self.agent.as_ref().map_or(0, |run| run.id));
    }

    pub(in crate::island) fn on_shell(&mut self) {
        let Some(outcome) = shell::take_outcome() else { return };
        let (serial, name) = match &outcome {
            shell::Outcome::Reviewed { run, tool, .. } | shell::Outcome::Ran { run, tool, .. } => (*run, tool.name()),
        };
        if self.agent.as_ref().is_none_or(|run| run.id != serial) {
            crate::log_info!("Assistant: dropped the result of {name} for a request that is no longer running");
            return;
        }
        self.clear_notice();
        match outcome {
            shell::Outcome::Reviewed { tool, verdict, .. } if verdict.safe => self.handle_tool(tool, true, false),
            shell::Outcome::Reviewed { verdict, .. } => {
                let text = format!("Blocked by the check: {}", verdict.reason);
                self.fail_by_voice(&text);
                self.end_wake_session();
            }
            shell::Outcome::Ran { tool, result, .. } => {
                match &result {
                    Ok(text) if matches!(tool, Tool::ReadFile(_)) => {
                        crate::log_info!(
                            "Assistant tool {} finished, {} characters",
                            tool.name(),
                            text.chars().count()
                        );
                    }
                    Ok(text) => crate::log_info!("Assistant tool {} finished: {}", tool.name(), brief(text, 300)),
                    Err(why) => crate::log_warn!("Assistant tool {} failed: {}", tool.name(), brief(why, 300)),
                }
                let Some(run) = self.agent.as_mut() else {
                    return match result {
                        Ok(text) => {
                            let short: String =
                                text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
                            self.notify_for(Notice::Ai, '\u{E8F2}', &short, 8000);
                        }
                        Err(why) => self.fail_by_voice(&why),
                    };
                };
                if result.is_ok() && tool.external() {
                    run.tainted = true;
                }
                run.did_tool(tool.name(), tool.detail(), tool.signature(), result.is_ok());
                if run.direct {
                    let text = result.unwrap_or_else(|why| {
                        self.cue(sounds::Cue::Error);
                        tr(&why).into_owned()
                    });
                    if let Some(run) = self.agent.as_mut() {
                        run.said(&serde_json::json!({"action":"answer", "text":text}).to_string());
                    }
                    self.say(&text);
                    return self.end_wake_session();
                }
                run.result(tool.name(), &result);
                self.call_agent();
            }
        }
    }

    fn handle_command(&mut self, command: voicecmd::Command, confirmed: bool) {
        if let voicecmd::Command::Tool(tool) = command {
            return self.handle_tool(tool, confirmed, confirmed);
        }
        if let Some(cap) = voicecmd::cap_of(&command) {
            match self.settings.assistant.perms.command_level(cap, false) {
                Level::Off => {
                    let text = format!("{}: {}", tr(cap.title()), tr("off in the Assistant settings"));
                    return self.fail_by_voice(&text);
                }
                Level::Ask if !confirmed && !matches!(command, voicecmd::Command::Power(_)) => {
                    let text = self.confirm_text(&format!("{}?", tr(cap.title())));
                    self.pending_ask = Some((command, std::time::Instant::now()));
                    return self.ask_user('\u{E897}', &text);
                }
                _ => {}
            }
        }
        match command {
            voicecmd::Command::Tool(_) => {}
            voicecmd::Command::Remember(text) => {
                let mut notes = memory::load();
                if memory::add(&mut notes, &text) && memory::save(&notes).is_ok() {
                    self.show_notice_for('\u{E73E}', "Remembered", 1800);
                } else {
                    self.show_notice_for('\u{E711}', "Already known", 1800);
                }
            }
            voicecmd::Command::Forget(what) => {
                let mut notes = memory::load();
                let dropped = memory::forget(&mut notes, &what);
                if dropped > 0 && memory::save(&notes).is_ok() {
                    self.show_notice_for('\u{E74D}', &format!("{} {dropped}", tr("Forgot")), 1800);
                } else {
                    self.show_notice_for('\u{E711}', "Nothing like that in memory", 2200);
                }
            }
            voicecmd::Command::Media(what) => {
                media::control(match what {
                    voicecmd::Media::PlayPause => media::Control::PlayPause,
                    voicecmd::Media::Next => media::Control::Next,
                    voicecmd::Media::Previous => media::Control::Previous,
                });
                self.show_notice_for('\u{E768}', "Done", 1500);
            }
            voicecmd::Command::Control(what, wanted) => {
                let current = match what {
                    voicecmd::Toggle::Mic => self.ui.mic_muted.map(|muted| !muted),
                    voicecmd::Toggle::WiFi => self.ui.radios.wifi,
                    voicecmd::Toggle::Bluetooth => self.ui.radios.bluetooth,
                    voicecmd::Toggle::Theme => Some(self.ui.dark),
                    voicecmd::Toggle::Lock => None,
                };
                if wanted.is_some() && wanted == current {
                    return self.show_notice_for('\u{E73E}', "Already like that", 1800);
                }
                self.run_ctl(match what {
                    voicecmd::Toggle::Mic => view::Ctl::Mic,
                    voicecmd::Toggle::WiFi => view::Ctl::WiFi,
                    voicecmd::Toggle::Bluetooth => view::Ctl::Bluetooth,
                    voicecmd::Toggle::Theme => view::Ctl::Theme,
                    voicecmd::Toggle::Lock => view::Ctl::Lock,
                });
                self.show_notice_for('\u{E73E}', "Done", 1500);
            }
            voicecmd::Command::Remind(phrase) => match remind::parse(&phrase, chrono::Local::now()) {
                Some(parsed) => {
                    let text = if parsed.text.is_empty() { "Reminder".to_string() } else { parsed.text };
                    self.store_reminder(parsed.due, &text);
                    self.show_notice_for(
                        '\u{E916}',
                        &format!("{} · {}", remind::describe(parsed.due, chrono::Local::now()), text),
                        3500,
                    );
                }
                None => self.notify_for(Notice::Error, '\u{E783}', "No time in that command", 3500),
            },
            voicecmd::Command::Timer(secs) => {
                let due = chrono::Local::now() + chrono::Duration::seconds(i64::from(secs));
                self.store_reminder(due, &crate::ui::i18n::tr("Timer"));
                let length = view::duration_text(i64::from(secs));
                self.show_notice_for('\u{E916}', &format!("{} · {length}", crate::ui::i18n::tr("Timer")), 3500);
            }
            voicecmd::Command::Open(name) => {
                let address = voicecmd::is_web_address(name.trim());
                if address {
                    cmd::shell_open(name.trim(), None);
                    self.show_notice_for('\u{E774}', "Opened", 1800);
                } else if !self.open_by_voice(&name) {
                    self.open_missed(&name);
                }
            }
            voicecmd::Command::OpenUrl(url, browser) => {
                let started = match browser.program() {
                    Some(program) => cmd::shell_run(program, &url),
                    None => false,
                };
                if !started {
                    cmd::shell_open(&url, None);
                }
                self.show_notice_for('\u{E774}', &url, 2500);
            }
            voicecmd::Command::Island { what, name } => {
                let result = self.island_info(&what, &name);
                if let Some(run) = self.agent.as_mut() {
                    run.result("island", &result);
                }
                self.call_agent();
            }
            voicecmd::Command::Note { name, text, append } => match self.write_note(&name, &text, append) {
                Ok(done) => self.show_notice_for('\u{E70B}', &done, 2200),
                Err(why) => self.fail_by_voice(&why),
            },
            voicecmd::Command::Discord(op) => {
                let (on, glyph, text) = match op {
                    voicecmd::DiscordOp::Mute(on) => {
                        discord::set_mute(on);
                        (on, '\u{E720}', if on { "Muted in Discord" } else { "Unmuted in Discord" })
                    }
                    voicecmd::DiscordOp::Deafen(on) => {
                        discord::set_deaf(on);
                        (on, '\u{E7F6}', if on { "Deafened in Discord" } else { "Undeafened in Discord" })
                    }
                };
                crate::log_info!("Assistant: Discord {op:?}, on {on}");
                self.show_notice_for(glyph, text, 1800);
            }
            voicecmd::Command::Screenshot => {
                let shot = capture::screen_png(self.hwnd, 1280);
                let size = shot.as_ref().map_or(0, |png| png.len() / 1024);
                crate::log_info!("Assistant: screenshot taken, {size} KB");
                if let Some(run) = self.agent.as_mut() {
                    match shot {
                        Some(png) => {
                            run.tainted = true;
                            let picture = Attachment {
                                name: "screen.png".into(),
                                kind: attach::Kind::Image,
                                mime: "image/png",
                                data: std::sync::Arc::new(png),
                            };
                            run.saw("Here is the screenshot of the main screen.", picture);
                        }
                        None => run.result("screenshot", &Err("The screen could not be captured".into())),
                    }
                }
                self.call_agent();
            }
            voicecmd::Command::Volume(what) => {
                let level = match what {
                    voicecmd::Volume::Set(percent) => {
                        volume::set(f32::from(percent) / 100.0);
                        volume::get()
                    }
                    voicecmd::Volume::Change(delta) => volume::step(f32::from(delta) / 100.0),
                    voicecmd::Volume::Mute(muted) => {
                        volume::set_muted(muted);
                        volume::get()
                    }
                };
                let text = match (what, level) {
                    (voicecmd::Volume::Mute(true), _) => tr("Muted").into_owned(),
                    (_, Some(level)) => format!("{} {}%", tr("Volume"), (level * 100.0).round() as u32),
                    (_, None) => tr("Done").into_owned(),
                };
                self.show_notice_for('\u{E767}', &text, 1800);
            }
            voicecmd::Command::Power(what) => {
                self.pending_power = Some((what, std::time::Instant::now()));
                let (glyph, question) = match what {
                    voicecmd::Power::Shutdown => ('\u{E7E8}', "Shut down?"),
                    voicecmd::Power::Restart => ('\u{E777}', "Restart?"),
                    voicecmd::Power::Sleep => ('\u{E708}', "Sleep?"),
                };
                let text = self.confirm_text(question);
                self.ask_user(glyph, &text);
            }
            voicecmd::Command::Confirm => {
                let asked = self.pending_ask.take().filter(|(_, at)| at.elapsed().as_secs() <= voicecmd::CONFIRM_SECS);
                if let Some((command, _)) = asked {
                    return self.handle_command(command, true);
                }
                let fresh =
                    self.pending_power.take().filter(|(_, at)| at.elapsed().as_secs() <= voicecmd::CONFIRM_SECS);
                match fresh {
                    Some((voicecmd::Power::Shutdown, _)) => self.run_ctl(view::Ctl::PowerOff),
                    Some((voicecmd::Power::Restart, _)) => self.run_ctl(view::Ctl::Restart),
                    Some((voicecmd::Power::Sleep, _)) => self.run_ctl(view::Ctl::Sleep),
                    None => self.notify_for(Notice::Error, '\u{E783}', "There is nothing to confirm", 3000),
                }
            }
            voicecmd::Command::Cancel => {
                self.pending_power = None;
                self.pending_ask = None;
                self.cancel_voice_input();
                self.show_notice_for('\u{E711}', "Cancelled", 1500);
            }
            voicecmd::Command::Answer(text) => self.say(&text),
            voicecmd::Command::Ask(text) if self.settings.assistant.perms.enabled && self.agent.is_some() => {
                self.ask_user('\u{E8F2}', &text);
            }
            voicecmd::Command::Ask(text) => self.say(&text),
            voicecmd::Command::Unknown => {
                crate::log_info!("Assistant: the model took the input for noise and stayed silent");
                self.show_notice_for('\u{E711}', "Did not catch that", 1800);
            }
        }
    }

    pub(in crate::island) fn on_transcribed(&mut self) {
        let Some(answer) = ai::take_transcript() else { return };
        if self.pet.voice_phase != VoicePhase::Transcribing {
            return;
        }
        self.voice_phase(VoicePhase::Idle);
        self.clear_notice();
        match &answer {
            Ok(text) if self.wake_session || self.agent.is_some() => {
                crate::log_info!("Assistant heard: {}", brief(text, 300));
                if let Some(why) = self.noise_in(text) {
                    crate::log_info!("Assistant: ignored what it heard, it was {why:?}");
                    return self.end_wake_session();
                }
            }
            Err(why) => crate::log_warn!("Speech recognition failed: {}", brief(why, 200)),
            Ok(_) => {}
        }
        match answer {
            Ok(text) if self.agent.as_ref().is_some_and(|r| r.phase == Phase::Listening) => {
                let wakes = voicecmd::wake_words(&self.settings.ai.voice_wake);
                let reply = voicecmd::strip_wake(&text, &wakes).filter(|r| !r.is_empty()).unwrap_or(text);
                if self.agent.as_ref().is_some_and(|run| run.direct) {
                    let reply = reply.trim_matches(|c: char| c.is_whitespace() || ".!?,".contains(c)).to_lowercase();
                    if let Some((command, at)) = self.pending_ask.take()
                        && at.elapsed().as_secs() <= voicecmd::CONFIRM_SECS
                        && matches!(
                            reply.as_str(),
                            "да" | "да подтверждаю"
                                | "да, подтверждаю"
                                | "подтверждаю"
                                | "yes"
                                | "confirm"
                        )
                    {
                        return self.handle_command(command, true);
                    }
                    self.show_notice_for('\u{E711}', "Cancelled", 1500);
                    return self.end_wake_session();
                }
                if let Some(mut run) = self.agent.take() {
                    if run.following {
                        self.keep_run(&run);
                        run.did.clear();
                        run.done.clear();
                        run.spoken.clear();
                        run.request = reply.chars().take(600).collect();
                        run.follow_ups += 1;
                        run.following = false;
                    }
                    run.steps = 0;
                    run.retries = 0;
                    run.heard(&reply);
                    self.agent = Some(run);
                }
                self.call_agent();
            }
            Ok(text) if self.wake_session => {
                let wakes = voicecmd::wake_words(&self.settings.ai.voice_wake);
                let command = voicecmd::strip_wake(&text, &wakes).unwrap_or(text);
                self.start_voice_command(&command);
            }
            Ok(text) => {
                let wakes = voicecmd::wake_words(&self.settings.ai.voice_wake);
                if let Some(command) = voicecmd::strip_wake(&text, &wakes) {
                    return self.start_voice_command(&command);
                }
                if self.settings.voice_paste {
                    self.clipboard.own_clip_seq = clips::sys::write_and_paste(self.hwnd, &text);
                } else {
                    clips::sys::write_text(self.hwnd, &text);
                }
                let short: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(46).collect();
                self.show_notice_for('\u{E73E}', &short, 1800);
            }
            Err(why) => {
                let short: String = why.chars().take(70).collect();
                self.quiet_or_fail(&short);
                self.end_wake_session();
            }
        }
    }
}

fn action_label(command: &voicecmd::Command) -> Option<String> {
    use voicecmd::Command as C;
    Some(match command {
        C::Media(_) => "media".into(),
        C::Control(what, _) => format!("control {what:?}").to_lowercase(),
        C::Volume(_) => "volume".into(),
        C::Remind(_) | C::Timer(_) => "reminder".into(),
        C::Open(name) => format!("open {name}"),
        C::OpenUrl(url, _) => format!("open_url {url}"),
        C::Power(what) => format!("power {what:?}").to_lowercase(),
        C::Remember(_) => "remember".into(),
        C::Forget(_) => "forget".into(),
        C::Screenshot => "screenshot".into(),
        C::Note { name, .. } => format!("note {name}"),
        C::Discord(op) => format!("discord {op:?}").to_lowercase(),
        C::Island { .. } => return None,
        C::Tool(_) | C::Confirm | C::Cancel | C::Answer(_) | C::Ask(_) | C::Unknown => return None,
    })
}

const MAX_RETRIES: usize = crate::services::assistant::agent::MAX_RETRIES;

const REPEAT_HINT: &str = "You already ran exactly this and its result is above. Use that result, change the \
command, or finish with an answer.";

const RETRY_HINT: &str = "That reply was not a valid action. Reply again with exactly one JSON object from the list \
(or an array of independent ones), starting with \"thought\", and nothing else.";

fn brief(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max { flat } else { format!("{}…", flat.chars().take(max).collect::<String>()) }
}
