<div align="center">

# Night Island

A lightweight Dynamic Island for Windows 10/11, written in Rust.

[English](#english) · [Русский](#русский)

</div>

## English

A small floating pill at the top of the screen that grows into a panel on hover. It shows what is playing, what you copied, what is downloading and what the weather is, and keeps a few tools one click away. It draws only when something changes, so it stays out of the way: about 3 MB executable, no polling without a reason.

### Features

- Media card with cover, progress and optional synced lyrics; Spotify and Discord integrations
- Clipboard history with pinning and search, a file shelf, notes with folders and reminders
- Weather, battery, notifications and download progress right on the pill
- Screen recording to MP4 or GIF, quick toggles (Wi-Fi, Bluetooth, theme, lock)
- Command bar on `Alt+Space`: apps, calculator, text tools, QR codes
- Themes, transparency and blur, free or docked position, global hotkeys
- Russian and English interface

### Guided setup and memory tools

The setup wizard offers a quick route (layout, essential toggles and Assistant permissions) and a detailed route with hands-on practice and voice-model downloads. You can switch routes without losing your choices. The miniature uses the real player, notification card and pet renderer with isolated sample data: playback controls, seeking, volume and pet dragging do not affect desktop apps. Choices apply only when you finish. Reopen the wizard from **Settings → About**.

On first activation, an untouched dock imports Windows taskbar shortcuts. **Import Windows taskbar pins** merges shortcuts into a configured dock without replacing its order; imported links are copied into Island's profile so unpinning them in Windows does not break them. This imports the taskbar's available `.lnk` files, not every packaged-app pin. The permanent add button is off by default; drag files to the dock or use its context menu. The button can be enabled in dock settings.

**Settings → System → Memory manager**, also available from the dock menu, releases resident pages of selected minimized desktop apps without closing them. Active apps and Windows components are skipped. Persistent exclusions and an optional two-minute observation show which apps regain memory. System free-memory figures can also change due to unrelated activity; trimming does not fix leaks or guarantee a performance improvement. Monitoring stops after the observation period when the manager is closed.

### App volume and Windows notifications

Left-click the **speaker on Home** to switch between system volume and the current player's application volume. Right-click it to unfold the application mixer; right-click again or press Escape to close it. The full panel is also available under **More → App mixer**. Paused applications remain available while they have an audio session. The mixer refreshes only while open.

Open **More → Notifications → Connect** and allow notification access if Windows asks. Enable **Do not disturb** through **Windows settings** to suppress ordinary Windows banners. Hover over a notification to open a Home preview occupying one, two or four tiles, as selected in notification settings. Its expand button opens the full, scrollable message. **Open sender app** is always available in the full message; an optional setting also makes the notification itself open the sender. Existing Windows notifications are never removed or replayed on connection. Notification contents stay in memory.

Windows priority exceptions, reminders and alarms can still display their own banners; adjust these in [Windows notification settings](https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows). Notification quick actions are not implemented yet. On Windows versions where notification events are unavailable to unpackaged apps, the enabled listener checks for new notifications about once per second. Disable the feature to stop these checks.

The **Setup guide** inside Notifications explains all three steps: notification access, hiding an application's Windows banners while keeping its notification-center entries, and enabling **Use Windows notifications** in Telegram's notification settings. Telegram's custom popup windows do not pass through the Windows listener.

Cards and history display the sender/chat image with a small application badge. A read-only fallback to the current user's Windows notification database recovers images and original activation arguments unavailable through the public listener. Records must match notification ID, sender ID and full text; schema changes or locked databases safely fall back to the listener. Local paths, including Discord's mixed path separators, are supported. Discord CDN avatar requests are bounded and use an allowlist; decoded images are cached in memory. Missing images fall back to the app icon.

Opening a notification uses its original registered Windows notification callback and arguments, or a supported sender-owned protocol link. The sender's registered shortcut is the fallback. Some applications provide no destination in their toast, so opening a specific conversation depends on their activation handler. No message text is interpreted as a command, and notification quick actions/replies are not replayed.

### Header customization

Use **More → Customize header…** to add a tab without removing another, explicitly replace a tab, or move a pinned tab back to More. Up to five additional tabs have full labels; more than five switch to compact circular icons with tooltips. Tabs that do not fit remain in More. Liquid deformation is decorative: only the original island body and its attached character capture mouse input.

### Yandex Music: My Wave

Open **More → My Wave → Connect Yandex Music** or **Settings → Integrations → Yandex Music**, copy the code and confirm it at [ya.ru/device](https://ya.ru/device). The island never asks for your password. The access token is encrypted for your Windows account; **Sign out** stops playback and removes the local token.

The Home card and My Wave share playback, seeking and track colors. Yandex cover metadata supplies the background, mini-player, accent and text palette; missing colors fall back to the cover. Colors transition smoothly with readable contrast. Select **My Wave · Settings** on Home or **Open main player** in My Wave to move between them without restarting playback.

Start your personal wave and adjust activity, variety, mood and language with the arrows. Available choices come from Yandex. Changes apply after the current track; **Next** applies them immediately. Playback continues when the island is collapsed, with cover colors, progress and controls on Home and the desktop companion. **Stop wave** releases control to your other music apps. Automatic playback on application startup is opt-in.

This integration uses the unofficial Yandex Music API and native Windows audio playback (MP3). An eligible subscription and region are required. Protocol behavior was researched in [yamusic-md3](https://github.com/yaskorinov/yamusic-md3); the implementation uses Night Island's native UI and workers. Account-specific playback has been confirmed by the user; automated checks cover device authorization endpoints, API requests, queue metadata, native audio and UI rendering.

### Desktop dock

An optional bottom dock shares Island's process and graphics device. Enable it in **Settings → Desktop dock** after closing another dock. It includes pinned apps and files, running-window indicators, click to launch/restore/minimize, a window list on right-click, pinned-icon reordering and file drops. Scroll when the app list exceeds the screen width.

Settings control icon size, hover magnification, native backdrop blur, tint hue/saturation/brightness, opacity, bottom margin, monitor, auto-hide and economy or smooth motion. Blur and plain materials remember separate opacity values; unsupported systems keep the configured tint without the blur effect. Icons load asynchronously at the physical size needed for DPI and zoom, preserve their aspect ratio and alpha, and reuse cached GPU bitmaps. Open Island settings appear in the running-app list while helper windows remain excluded.

Hover cards prepare controls as soon as the pointer reaches an app, reuse a short app-specific cache and receive updates immediately. Media metadata loads without blocking application volume or Discord voice state. The card inherits the dock's blur, tint and opacity; live window thumbnails stay sharp. The blur surface hides with the card and follows capture privacy.

An optional **Settings → Top panel** adds compact companion blocks around the existing island on the primary display. Choose the active app, Wi-Fi, Bluetooth, system volume and battery/power blocks independently. App actions restore/maximize, minimize, close normally or switch windows. System blocks open small Island-style cards; volume links to the app mixer. Colours follow the Island theme, opening fades respect reduced motion, and unchanged widgets stop repainting. Right-click any block to open its settings.

The top panel can reserve space above maximized windows and releases it when disabled. The island and its notifications stay inside that strip; side blocks yield when they overlap an expanded island or notification. Fullscreen hiding is independent of the bottom dock. The panel supports capture privacy and stays out of Alt+Tab and the dock's running-app list. Existing installations keep the feature off until enabled.

The top panel supports edge reveal: hover the top three screen pixels to show the side blocks, then move away for 650 ms to hide them. Cards and the native tray keep the panel available during interaction; fullscreen suppression still takes priority. Autohide releases the reserved top strip and does not change the island's independent visibility setting. The tray arrow expands the right-hand pill to the left over 240 ms. Up to six icons fit per page; the count adapts to space beside the central island and preserves the reserved window controls. Remaining icons use the overflow button. Width, position and content are presented together, using one reusable backing bitmap throughout expansion. Left-click invokes the real icon; right-click requests its native menu. Names appear on hover. Reduced motion disables the transition. Explorer supplies the live list and actions through accessibility; Windows' stored PNG snapshots supply the images. Unknown images use a fallback glyph. The list refreshes in the background on expansion; startup preloads the first snapshot and subsequent opens animate cached entries immediately. Icon PNGs are cached for one minute. Closing does not wait for shell or system-data polling. Regular actions access the existing overflow host without toggling Explorer's chevron or changing foreground focus; native menu cleanup waits until the menu is dismissed. Verified on the current Windows 11 tray; other shell versions may show the help card.

The tray opens automatically even while Island hides the Windows taskbar. Explorer's taskbar and overflow host are briefly made available behind empty window regions, then their regions and visibility are restored. A newly created overflow host may briefly appear during its first initialization. Individual application-icon switches do not need changing. If Explorer cannot open the menu, a persistent help card offers Windows taskbar settings and a retry button instead of assuming the hidden-icon menu is disabled.

Alt + right-click on the Island settings entry in the dock toggles Island's capture exclusion. **Settings → Desktop dock → Experimental capture protection** enables the same gesture for supported 64-bit browsers, messengers, editors and players running without administrator rights. A temporary, thread-specific hook loads Island's embedded helper into the selected process; it applies and verifies Windows display affinity. A lock badge marks protected windows. Repeat the gesture to restore their previous affinity. This affects existing windows only: newly opened windows need another explicit action. Games, unknown executables, elevated processes and 32-bit targets are excluded; detected anti-cheat processes block the integration. The setting is off by default. Tests verify exclusion from an actual desktop capture and restoration using a disposable process, not every supported application. Windows display affinity is not a guarantee against every capture method.

Right-click opens with actions for the selected application: show/open, preview, window selection, restore/minimize and normal close. More options contains pinning, file location/properties, executable paths and dock/Island settings. Long menus scroll within the current monitor and support keyboard navigation.

Fullscreen and borderless apps hide the dock on their monitor; ordinary maximized windows do not. For windowed games, use **Dock behavior → Hide dock while this app is active**; rules can be removed in dock settings. Hidden dock windows do not intercept the pointer or run animation/icon-refresh work. Alt+Tab restores the dock without activating it. Game compatibility is checked with native window fixtures, not with every game or anti-cheat. Animation frames stop at rest, Windows reduced-motion settings are respected, and disabling the dock releases its window and timers. The Windows taskbar settings link lets you enable its auto-hide; The replacement setting hides the Windows taskbar while the dock is enabled. A separate guardian restores its original visibility and autohide setting when Island exits, disables the dock or disconnects unexpectedly; Explorer keeps running.

**Import applications from MyDockFinder** reads `ico.ini` from the default Steam installation, or asks for the file when it is elsewhere. It merges application paths in order without copying MyDockFinder binaries, themes or icon packs. Icons come from Windows. Notification badges and third-party window-minimization effects are not included. The upper panel remains Island.

Hover briefly over a running app to open its live Windows thumbnail, switch between its windows, open or close a window, and use the controls that app exposes. Media controls target the hovered application and selected session, even when Island shows another player; app volume/mute affects only its audio sessions. Discord microphone/deafen controls require the existing authorized Discord integration and a confirmed voice-settings connection to that exact Discord client. Unavailable controls stay disabled; the popup links to integration settings when disconnected. Hover cards reuse Island’s popup panel, theme and rounded surfaces. Discord places the voice channel and participant status above a compact window preview, with circular mute/deafen controls; app audio uses the same slider style as Home. Previews never restore or activate a source window on hover; minimized apps may supply a cached image, and protected/unavailable sources show a fallback. Escape or leaving the card dismisses it. The hover option can be disabled in dock settings; **Window preview and controls** remains available on right-click.

Dock contours preserve their antialiased edge pixels at fractional DPI, and switching/minimizing windows triggers a single delayed backdrop refresh rather than a continuous capture loop.

### Interface motion

Opening uses one compositor timeline for the silhouette, artwork and content. The body stays within its final bounds, content appears once there is room, and a return to the closing island continues from its current position and velocity. Hover delay starts once on entry, rather than restarting with every mouse movement. During the transition, transparent areas pass clicks through. Opening and closing speed and the motion curve remain adjustable under Animations; Windows reduced motion is respected.

Dock magnification follows the pointer with damped motion, without a second delayed layout pass. Reordering has its own movement, tooltips fade, and mouse events share a scheduled frame instead of repainting for every input event. Hover previews fade their content, live thumbnail and blur together, retain the previous preview while moving between apps, and reopen smoothly if the pointer returns during dismissal. Dock animations and economy mode remain independently configurable. Animation timers stop at rest.

Home action tiles, playback buttons, header selection and control tiles use short hover transitions. Pressed Home buttons compress slightly; settings switches slide their thumb and segmented controls move their selection. Repeated input retargets from the currently visible value. The feedback timer stops when transitions finish.

When Home is opened from the music pill, its cover moves between the compact slot and the player card; closing reverses that movement. Changing a track or player fades the artwork, replaces the labels in two phases for readability and blends the card tint. Rapid track changes preserve the current visual mixture. Cover motion is cleaned up on hiding, layout changes and navigation. New motion respects Windows' animation setting.

### Desktop pets

Drag a character out of the collapsed island to release it; drag it back over the island to dock it. Pets breathe, follow the pointer smoothly, blink, react to gentle pointer strokes, stretch while carried and settle when released. Bear joins the character list. Music controls and the mini AI chat remain available beside every detached character, including custom pets.

**Settings → Island → Pet animation quality** offers reduced motion, economical (up to 20 FPS), balanced (up to 30 FPS) and smooth (up to 60 FPS). The pet has its own frame budget, lowers its refresh rate at rest and while sleeping, and stops drawing in gaming mode.

**Pet walks, rests and sleeps on its own** enables short walks along the height where you leave the detached pet, sitting breaks and occasional naps. Pets stay inside the current monitor’s working area. Approaching, dragging, using the player or opening either AI chat pauses roaming and wakes the pet. There is a short pause after interaction before the next walk. Disabling autonomy preserves manual dragging and pointer reactions; reduced motion also prevents walking. Movement never activates the pet window or saves settings on each frame.

Open **Pet studio** in either AI chat. Describe a pet or select **Pet photo**, then send your request using your configured AI provider. The model produces a stylized animated character with editable colors, body, markings and accessories. Review the preview and select **Apply pet**; further messages revise that design. Photo input requires a vision-capable model. **Text only** excludes the attached photo and lets you describe the desired appearance instead. The chosen pet survives restarts; generated designs cannot contain code or external assets. The studio shows a live portrait, the selected photo and a color palette, with distinct preview, applied and error states. Its animation follows the pet quality setting and pauses when hidden; the mini chat also pauses it when unfocused. Rounded silhouettes, soft shading and expressive eyes carry across both the island and detached pets.

### Pet speech extension

`src/ui/pet_behavior.rs` exposes time-based activity transitions as `BehaviorEvent` values. `src/ui/pet_speech.rs` accepts typed `SpeechCue` values (`Quiet`, `Listening`, `Thinking`, `Speaking { amplitude }`) and smooths a bounded mouth envelope. The controller in `src/island/character.rs` derives listening/thinking cues from both chats; a future voice adapter can set `Pet.speech_cue` while playing audio and restore `Quiet` when finished or cancelled. Speaking keeps the pet attentive and stops roaming. `Motion.mouth` drives mouth shapes (or eye modulation for Eyes) and honors reduced motion. No microphone capture, speech synthesis or extra AI requests are enabled by this foundation.

### Build

Requires Windows, a recent stable Rust toolchain (edition 2024) and a C compiler (MSVC Build Tools or mingw-w64).

```
cargo build --release
```

The executable is `target/release/night-island.exe`. Settings and data live in `%APPDATA%\NightIsland`.

### Contributing

Issues and pull requests are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md). Security reports: [SECURITY.md](SECURITY.md).

### License

[MIT](LICENSE). Bundled icons keep their own licenses, see `assets/material/LICENSE` and `assets/weather/src/LICENSE`.

## Русский

Небольшая плавающая «пилюля» у верхнего края экрана, которая раскрывается в панель при наведении. Показывает, что играет, что скопировано, что скачивается и какая погода, а ещё держит под рукой несколько инструментов. Рисует только при изменениях, поэтому не мешает системе: exe около 3 МБ, опросов без причины нет.

### Возможности

- Карточка плеера с обложкой, прогрессом и синхронным текстом песни; интеграции со Spotify и Discord
- История буфера обмена с закреплением и поиском, полка для файлов, заметки с папками и напоминания
- Погода, батарея, уведомления и ход загрузок прямо на пилюле
- Запись экрана в MP4 или GIF, быстрые переключатели (Wi-Fi, Bluetooth, тема, блокировка)
- Командная строка на `Alt+Space`: приложения, калькулятор, текстовые утилиты, QR-коды
- Темы, прозрачность и размытие, свободное или закреплённое положение, глобальные горячие клавиши
- Интерфейс на русском и английском

### Знакомство и управление памятью

В мастере есть быстрый маршрут — пресет, основные переключатели и разрешения ассистента — и подробный, с практикой и загрузкой голосовых моделей. Маршрут можно сменить без потери выбранных параметров. Миниатюра использует настоящий плеер, карточку уведомлений и отрисовку питомца с отдельными демонстрационными данными. Кнопки плеера, перемотка, громкость и перетаскивание питомца не влияют на приложения рабочего стола. Настройки применяются в конце. Повторный запуск — **Настройки → О программе**.

При первом включении ненастроенный док импортирует ярлыки панели задач Windows. Кнопка **Импортировать ярлыки панели задач Windows** добавляет их и в уже настроенный док, сохраняя существующий порядок. Копии ссылок хранятся в профиле Island, поэтому открепление от панели задач Windows их не ломает. Импортируются доступные файлы `.lnk`; некоторые закреплённые приложения Microsoft Store могут отсутствовать. Постоянный «+» по умолчанию скрыт: добавляй приложения перетаскиванием или через ПКМ. Кнопку можно вернуть в настройках дока.

**Настройки → Система → Управление памятью** также открывается из меню дока. Очистка освобождает резидентные страницы выбранных свёрнутых приложений, сохраняя сами процессы. Активные приложения и компоненты Windows пропускаются. Исключения запоминаются; наблюдение в течение двух минут показывает повторный рост расхода. Свободная память системы зависит и от других процессов: очистка не исправляет утечки и не гарантирует ускорение. Когда окно закрыто, после периода наблюдения опрос прекращается.

### Громкость приложений и уведомления Windows

**ЛКМ по динамику на главной** переключает системную громкость и громкость приложения текущего плеера. **ПКМ** раскрывает микшер; повторный ПКМ или Escape закрывает его. Полная панель остаётся в **Ещё → Микшер приложений**. Приложение остаётся в списке на паузе, пока у него есть звуковой сеанс. Микшер обновляется только при открытой панели.

Открой **Ещё → Уведомления → Подключить** и разреши доступ, если Windows его запросит. Через **Параметры Windows** включи **«Не беспокоить»**, чтобы скрыть обычные системные баннеры. Наведение на уведомление раскрывает его на главной: размер в одну, две или четыре плитки выбирается в настройках уведомлений. Кнопка разворачивания открывает полный текст с прокруткой. **«Открыть приложение»** доступно всегда в полном уведомлении; отдельная настройка позволяет переходить в приложение и по клику на саму карточку. Старые уведомления при подключении не показываются заново и не удаляются из центра Windows. Содержимое хранится только в памяти.

Приоритетные исключения, напоминания и будильники могут показывать свои баннеры — их можно настроить в [параметрах уведомлений Windows](https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows). Быстрые действия пока не добавлены. Если Windows не предоставляет события уведомлений приложению без установочного пакета, при включённой функции проверка выполняется примерно раз в секунду. Отключение функции останавливает проверки.

В уведомлениях есть вкладка **«Как настроить»**: доступ к уведомлениям, отключение баннеров Windows при сохранении записей в центре и включение **«Системного окна Windows» / «Использовать уведомления Windows»** в настройках Telegram. Собственные всплывающие окна Telegram не попадают в системный обработчик.

Карточки и история показывают фото отправителя/чата со значком приложения. Недостающие изображения и аргументы открытия берутся из базы уведомлений текущего пользователя Windows, доступ к которой выполняется только для чтения. Проверяется совпадение ID уведомления, приложения и полного текста. Если база недоступна или её формат изменился, продолжает работать системный обработчик. Поддерживаются локальные пути, включая смешанные разделители Discord, и ограниченная загрузка аватарок с CDN Discord. Готовые картинки сохраняются в памяти; при отсутствии изображения остаётся значок приложения.

Переход использует зарегистрированный обработчик уведомлений Windows с исходными аргументами или поддерживаемую ссылку самого приложения. Запасной вариант — зарегистрированный ярлык отправителя. Некоторые приложения не передают адрес назначения: открытие конкретного чата зависит от их обработчика. Текст сообщений не выполняется как команда. Быстрые действия и ответы не воспроизводятся.

### Настройка шапки

В **Ещё → Настроить шапку…** можно добавить вкладку, явно заменить другую или вернуть закреплённую вкладку в «Ещё». До пяти вкладок помимо главной показываются полные подписи; при большем количестве — круглые кнопки с иконками и подсказками. Не поместившиеся вкладки остаются в «Ещё». Тянущийся край декоративный: мышь перехватывают только исходное тело острова и прикреплённый персонаж.

### Яндекс Музыка: Моя волна

Открой **Ещё → Моя волна → Подключить Яндекс Музыку** или **Настройки → Интеграции → Яндекс Музыка**, скопируй код и подтверди вход на [ya.ru/device](https://ya.ru/device). Остров не запрашивает пароль. Токен шифруется для текущего пользователя Windows; **Выйти** останавливает музыку и удаляет сохранённый токен.

Главная карточка и «Моя волна» используют общий плеер, перемотку и палитру трека. Цвета фона, мини-плеера, акцента и текста приходят из Яндекса; недостающие вычисляются из обложки. Цвета сменяются плавно с сохранением читаемости. «Моя волна · Настроить» на главной и «К плееру» в волне переключают вкладки без перезапуска музыки.

Запусти свою волну и стрелками выбери занятие, характер, настроение и язык. Варианты загружаются из Яндекса. Новые настройки применяются после текущего трека; кнопка **Следующий** переключает сразу. Музыка играет и в свёрнутом острове: работают цвета обложки, прогресс, управление на главной и у персонажа. **Остановить** возвращает управление другим плеерам. Запуск волны вместе с островом включается отдельно.

Используется неофициальный API Яндекс Музыки и встроенный аудиоплеер Windows (MP3). Нужны подходящая подписка и доступность сервиса в регионе. Связь с API изучена по [yamusic-md3](https://github.com/yaskorinov/yamusic-md3), интерфейс и обработчики написаны для Night Island. Пользователь подтвердил работу персональной волны после входа; автоматические проверки покрывают получение кода, запросы API, данные очереди, воспроизведение Windows и отрисовку вкладки.

### Нижний док

Отдельная нижняя панель работает внутри процесса Island на общем графическом устройстве. Включается в **Настройки → Нижний док**, после закрытия другого дока. Поддерживает закреплённые приложения и файлы, индикаторы запущенных окон, запуск/возврат/сворачивание по клику, список окон по ПКМ, перестановку значков и добавление перетаскиванием. Если приложения не помещаются, список прокручивается колесом.

В настройках: размер, увеличение при наведении, системное размытие фона, оттенок/насыщенность/яркость панели, непрозрачность, отступ, монитор, автоскрытие и анимации 30/60 кадров/с. Размытая и обычная панели запоминают непрозрачность отдельно. Если системное размытие недоступно, сохраняется выбранный цвет панели. Иконки загружаются в фоне под масштаб Windows и увеличение, сохраняют пропорции и прозрачность, а готовые изображения переиспользуются на GPU. Открытые настройки Island видны в списке запущенных приложений; служебные окна туда не попадают.

Карточка готовит управление уже при наведении на значок, использует короткий отдельный кэш для каждой программы и сразу получает обновления. Загрузка названия трека не задерживает громкость и состояние Discord. Размытие, оттенок и прозрачность берутся из настроек дока; сам предпросмотр окна остаётся чётким. Фон скрывается вместе с карточкой и учитывает защиту от захвата экрана.

В **Настройки → Верхняя панель** можно включить небольшие блоки по краям основного экрана: активную программу, Wi-Fi, Bluetooth, системную громкость и питание. Каждый блок отключается отдельно. В карточке программы — восстановление/разворачивание, сворачивание, обычное закрытие и выбор окна; в карточке громкости — ползунок и переход в микшер приложений. Цвета соответствуют теме Island, появление плавное с учётом системного отключения анимаций, неизменившиеся блоки не перерисовываются. ПКМ по блоку открывает настройки панели.

Опция «Оставлять место над развёрнутыми окнами» освобождает верхнюю полосу и возвращает рабочую область при выключении панели. Сам остров и его уведомления используют эту полосу; боковые блоки уступают место раскрытому острову или уведомлению при пересечении. Скрытие в полноэкранных приложениях настраивается отдельно от нижнего дока. Поддерживается защита от захвата экрана; служебные окна не попадают в Alt+Tab и список программ дока. В существующих установках панель включается вручную.

Верхняя панель появляется при наведении к верхнему краю (3 пикселя) и скрывается через 650 мс после ухода курсора. Карточки и открытый системный трей удерживают её видимой; полноэкранный режим имеет приоритет. При автоскрытии верхний отступ не резервируется, видимость самого острова остаётся отдельной настройкой. Стрелка трея расширяет правый блок влево за 240 мс. На странице до шести значков; их число уменьшается, если рядом с центральным островом мало места. Остальные доступны по многоточию. Положение, размер и изображение меняются одним кадром, без пересоздания буфера при расширении. ЛКМ вызывает настоящее действие значка, ПКМ — его родное меню. При наведении показывается название. Изображения берутся из сохранённых Windows PNG значков; неизвестные получают запасной символ. Список заранее загружается при старте, при раскрытии сразу используется сохранённая копия и запускается фоновое обновление. PNG значков кэшируются на минуту. Сворачивание не ждёт Explorer и обновления системных данных. Обычные действия больше не переключают стрелку Explorer и не отбирают фокус; вспомогательное окно скрывается после закрытия родного меню. При отключённых анимациях переход мгновенный. Проверено на текущем трее Windows 11.

Трей открывается автоматически и при скрытой панели задач Windows. Island кратковременно делает элементы Explorer доступными за пустой областью окна, затем восстанавливает прежние области и видимость. При первой инициализации ещё не созданного меню Explorer оно может кратковременно появиться. Переключатели значков отдельных приложений менять не нужно. Если Explorer не открывает меню, появляется постоянная карточка с инструкцией, переходом в параметры Windows и повторной попыткой — без ошибочного утверждения, что меню скрытых значков выключено.

Alt + ПКМ по значку настроек Island в доке переключает исключение Island из захвата. **Настройки → Нижний док → Защита от захвата (эксперимент)** включает этот жест для поддерживаемых 64-битных браузеров, мессенджеров, редакторов и плееров без прав администратора. Временный перехватчик выбранного потока загружает встроенный модуль Island в процесс приложения; он применяет и проверяет защиту Windows. Защищённые окна отмечены замком. Повторное нажатие восстанавливает их прежнее состояние. Действие касается только уже открытых окон: новые нужно защищать отдельно. Игры, неизвестные программы, процессы с повышенными правами и 32-битные приложения исключены; обнаруженный античит блокирует интеграцию. По умолчанию режим выключен. На отдельном тестовом процессе проверены реальное исчезновение из захвата рабочего стола и восстановление, но не каждая поддерживаемая программа. Некоторые способы записи экрана могут игнорировать защиту Windows.

В меню ПКМ сначала идут действия с выбранной программой: показать/открыть, предпросмотр, выбор окна, восстановление/сворачивание и обычное закрытие. В «Дополнительно» перенесены закрепление, свойства и путь файла, настройки дока и Island. Длинные меню прокручиваются внутри монитора и поддерживают клавиатуру.

Полноэкранные приложения и игры без рамки скрывают док на своём мониторе; обычное развёрнутое окно — нет. Для игры в окне: **Поведение дока → Скрывать док в этом приложении**. Правило удаляется в настройках дока. Скрытый док не перехватывает мышь, не рисует анимации и не обновляет значки. После Alt+Tab возвращается без захвата фокуса. Проверки выполнены на нативных тестовых окнах, а не на всех играх и античитах. В покое отрисовка останавливается; учитывается отключение анимаций Windows. При выключении освобождаются окно и таймеры дока. По отдельной кнопке открываются параметры панели задач Windows для включения её автоскрытия; Опция «Заменять панель задач Windows» скрывает её на время работы дока. Отдельный процесс восстанавливает прежнюю видимость и настройку автоскрытия после выключения дока, выхода или потери связи с Island; Explorer продолжает работать.

Кнопка **Импорт приложений из MyDockFinder** читает `ico.ini` из стандартной папки Steam или предлагает выбрать файл. Приложения добавляются в исходном порядке без дубликатов; значки берутся из Windows. Бинарники, темы и наборы иконок MyDockFinder не копируются. Счётчики уведомлений и эффекты сворачивания чужих окон пока не реализованы. Верхнюю панель пока заменяет сам Island.

Задержи курсор над запущенным приложением: появится живой предпросмотр Windows, переключение между его окнами, открытие/закрытие и доступные быстрые действия. Музыкальные кнопки управляют выбранным плеером именно этого приложения, даже если Island показывает другой источник. Громкость и выключение звука относятся только к приложению. Для микрофона и звука Discord нужна подключённая интеграция с подтверждённым доступом к голосовым настройкам именно этого клиента Discord. Недоступные действия отключены; без подключения карточка предлагает перейти в настройки. Карточки используют общую панель, тему и скругления Island. У Discord сверху показаны голосовой канал, участники и круглые кнопки микрофона/наушников; ниже — компактное окно и ползунок громкости в стиле главной. Наведение не активирует и не разворачивает окно. Для свёрнутого окна Windows может показывать последний кадр; для защищённого или недоступного — заглушку. Escape или уход курсора закрывает карточку. Предпросмотр при наведении отключается в настройках дока; пункт **Предпросмотр и управление** остаётся в меню ПКМ.

Контур панели сохраняет сглаженные края при дробном масштабе Windows. После переключения и сворачивания окон фон обновляется один раз с небольшой задержкой, без постоянного захвата экрана.

### Анимации интерфейса

На главной плавно подсвечиваются плитки действий и кнопки плеера; нажатие слегка сжимает кнопку. В шапке плавно меняется выделение, а в настройках двигаются бегунок переключателя и выделение сегментов. Повторное действие продолжает переход из текущего состояния; после завершения таймер останавливается.

При открытии главной из музыкального острова обложка переходит в карточку плеера и возвращается при закрытии. При смене трека или источника обложка и оттенок карточки плавно меняются, а старая подпись уступает место новой без наложения текста. Быстрые переключения сохраняют текущую смесь изображений. Новые эффекты учитывают настройку анимаций Windows.

### Питомцы на рабочем столе

Вытяни персонажа из свёрнутого острова, чтобы выпустить его; перетащи обратно на остров, чтобы вернуть. Питомцы дышат, плавно следят за курсором, моргают, реагируют на поглаживание курсором, тянутся при переносе и мягко успокаиваются после отпускания. В списке появился медведь. Управление музыкой и мини-чат с ИИ доступны рядом с любым отдельным персонажем, включая собственного.

В **Настройки → Остров → Анимация питомца** доступны режимы без движения, экономный (до 20 FPS), сбалансированный (до 30 FPS) и плавный (до 60 FPS). Частота питомца настраивается отдельно от острова, снижается в покое и во сне. В игровом режиме отрисовка останавливается.

**Самостоятельные прогулки, отдых и сон** включают короткие прогулки на той высоте, где ты оставил отдельного питомца, отдых сидя и сон. Питомец остаётся в рабочей области текущего монитора. При приближении курсора, перетаскивании, работе с плеером или открытом ИИ-чате он останавливается и просыпается; после взаимодействия ждёт несколько секунд. Отключение самостоятельности сохраняет ручное перетаскивание и реакции на курсор. Режим «Без движения» также отключает прогулки. Перемещение не забирает фокус и не записывает настройки на каждом кадре.

Нажми **Питомец** в основном или мини-чате с ИИ. Опиши внешность или выбери **Фото** и отправь запрос настроенной модели. Получится стилизованный анимированный персонаж с выбранными цветами, формой, узором и аксессуарами. Проверь предпросмотр и нажми **Применить**; следующим сообщением можно изменить результат. Для фотографий нужна модель с поддержкой изображений. Кнопка **Без фото** исключает фотографию из запроса: вместо неё опиши внешность словами. Выбранный питомец сохраняется после перезапуска; результат модели не может содержать код или внешние ресурсы. В редакторе видны живой портрет, миниатюра выбранного фото и палитра; состояния предпросмотра, применения и ошибки различаются. Анимация следует настройке качества и останавливается в скрытом окне, а в мини-чате — также без фокуса. Скруглённые силуэты, мягкие тени и выразительные глаза сохраняются и в острове, и у отдельного питомца.

### Сборка

Нужны Windows, свежий стабильный Rust (edition 2024) и компилятор C (MSVC Build Tools или mingw-w64).

```
cargo build --release
```

Исполняемый файл: `target/release/night-island.exe`. Настройки и данные лежат в `%APPDATA%\NightIsland`.

### Участие

Issues и pull request приветствуются, см. [CONTRIBUTING.md](CONTRIBUTING.md). Сообщения об уязвимостях: [SECURITY.md](SECURITY.md).

### Лицензия

[MIT](LICENSE). Встроенные иконки сохраняют свои лицензии: `assets/material/LICENSE` и `assets/weather/src/LICENSE`.

The top panel follows Island's ghost mode (Ctrl + E by default), including configured opacity and click-through. **Keep window buttons accessible** reserves 144 logical pixels at the right edge by default; it can be disabled in top-panel settings.

Верхняя панель поддерживает режим прозрачности Island (по умолчанию Ctrl + E), включая пропуск кликов и выбранную прозрачность. Опция «Оставлять место для кнопок окна» по умолчанию освобождает справа 144 логических пикселя; её можно отключить в настройках верхней панели.

Dock shortcuts can be reordered by dragging pinned icons. The dragged icon lifts above its neighbours; other icons slide into their new positions before release. Capture loss cancels the change. Pinned entries stay on the left, including pinned apps that are running; unpinned running apps sit beyond a spaced divider. Settings can lock positions, hide the divider, disable animation or lower its frame rate. Right-click an entry → More options → Edit dock shortcut to change its name, launch target and icon, or reset the icon. Saving an unpinned entry pins it. Custom images load asynchronously through Windows thumbnails; a missing image falls back to the application icon. Existing shortcut settings migrate with an empty custom-icon field.

Optional top-panel controls add default-system-microphone mute status/toggle, the foreground app's input language with next-layout switching, and Windows Task View for windows and desktops. They appear beside the active application on the left and can each be enabled in top-panel settings; existing installations keep them off. Disabled controls do not poll microphone/layout state. Keyboard switching uses the standard Windows input-language request, which the target application can reject. Hover and press feedback eases independently; an animation toggle and Windows reduced-motion preference are respected. Native fixtures verify live reordering, custom PNG colors/transparency, shortcut-editor save/cancel, and UI action dispatch. The fixture does not toggle the user's actual microphone or open Task View.

Ярлыки дока переставляются перетаскиванием: выбранный значок приподнимается, соседи плавно раздвигаются ещё до отпускания мыши. Потеря захвата отменяет перестановку. Закреплённые ярлыки остаются слева, включая запущенные закреплённые программы; незакреплённые запущенные программы отделены полоской справа. В настройках можно заблокировать перестановку, убрать разделитель, отключить анимации или снизить частоту кадров. ПКМ по значку → «Дополнительно…» → «Изменить ярлык дока» открывает название, путь запуска и свой значок со сбросом к оригиналу. Сохранение незакреплённой программы закрепляет её. Свои изображения загружаются в фоне; если значок недоступен, остаётся иконка приложения. Старые настройки совместимы с новым полем значка.

Дополнительные блоки верхней панели: состояние и отключение системного микрофона, язык ввода активной программы с переключением раскладки, обзор окон и рабочих столов Windows. Они находятся слева рядом с активной программой и включаются по отдельности в настройках; по умолчанию новые блоки выключены. Выключенные блоки не опрашивают микрофон и раскладку. Программа может отклонить стандартный запрос Windows на смену языка ввода. Подсветка и нажатия плавные; есть отдельное отключение анимаций и поддержка системного уменьшения движения. Проверки не переключают реальный микрофон пользователя и не запускают Task View.
