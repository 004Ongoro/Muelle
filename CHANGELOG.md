# Changelog / Cambios

Each release lists its changes in English and in Spanish. The release workflow
copies the section of the version being published into the GitHub release.

Cada versión lista sus cambios en inglés y en español. El flujo de publicación
copia la sección de la versión que se publica a la release de GitHub.

## [Unreleased]

## [0.1.6]

### English

- Faster start with Windows: Muelle now starts from a task that runs at
  sign-in instead of the Run key, which Windows launches seconds after the
  desktop at low priority. Existing installs switch over on their own.
- The Windows taskbar no longer shows in full at every start: when Windows
  shuts down or restarts with Muelle set to start again, the taskbar stays
  hidden instead of being given back.
- Pinned apps imported from the Windows taskbar now keep its order and include
  Store apps (Media Player, WhatsApp...). The dock menu has "Add Apps Pinned to
  the Windows Taskbar" to bring in any that are missing.
- Uninstalling now also removes the start-with-Windows entry.
- Pinning or unpinning an app on the Windows taskbar (from Start, Explorer or
  the app's menu) now pins or unpins it in the dock too, right away, and also
  when it happened while Muelle was closed.
- Unchecking "Keep in Dock" also unpins the app from the Windows taskbar.
- Uninstalled apps leave the dock instead of staying as a broken icon.
- As on the Windows taskbar: middle click or Shift+click opens another window,
  Ctrl+Shift+click and a new menu item open the app as administrator, and the
  right edge of the menu bar (and the dock menu) shows the desktop.
- Menu bar: an input language indicator (ES, EN...) when more than one is
  installed; click it to switch. The clock follows time and time zone changes
  at once.
- Menu bar: microphone, camera and location icons while an app is using them,
  as in the Windows tray; click one to see which apps in Windows Settings.
- Switching virtual desktops updates the dock at once: it shows the apps of the
  desktop in front, as the Windows taskbar does.
- Windows 10 support (version 1903 or later). The dock used to close right
  after starting there. On Windows 10 the dock and menu bar are solid instead
  of glass, since it has no backdrop blur for desktop apps.
- The menu bar's "^" button opens Windows' hidden notification-area icons.
- A curved menu bar island shows usage for Claude, Codex, GitHub Copilot,
  Cursor, OpenCode Go, and Gemini or Antigravity while they are running. It can
  be turned off in Settings → Menu bar.
- Settings has a clearer layout with separate Dock, Windows, Menu bar and
  System sections, and its window can be resized or maximized.
- Minimized windows now fly into their app's dock icon by default. Live
  thumbnails on the right side of the dock remain available as an option.
- The active-app click option was removed: clicking the app in front minimizes
  it, as on the Windows taskbar.
- The dock's Muelle icon is sharper, and the Start button uses the Windows
  logo.
- Settings can turn off automatic update checks. The website adds privacy and
  code-signing policy pages.

### Español

- Arranque más rápido con Windows: Muelle ahora se inicia con una tarea al
  iniciar sesión en lugar de la clave Run, que Windows lanza segundos después del
  escritorio y con prioridad baja. Las instalaciones existentes cambian solas.
- La barra de tareas de Windows ya no aparece entera en cada arranque: al apagar
  o reiniciar con Muelle configurado para iniciarse, la barra se queda oculta en
  lugar de devolverse.
- Las apps fijadas que se importan de la barra de Windows conservan su orden e
  incluyen las de la Store (Reproductor multimedia, WhatsApp...). El menú del dock
  tiene «Añadir apps fijadas en la barra de Windows» para traer las que falten.
- Al desinstalar también se quita el inicio con Windows.
- Anclar o desanclar una app de la barra de tareas de Windows (desde Inicio, el
  Explorador o el menú de la app) ahora la ancla o desancla también en el dock, al
  momento, e incluso si pasó con Muelle cerrado.
- Quitar la marca de «Mantener en el Dock» también desancla la app de la barra de
  tareas de Windows.
- Las apps desinstaladas salen del dock en lugar de quedarse como icono roto.
- Como en la barra de Windows: clic central o Mayús+clic abre otra ventana,
  Ctrl+Mayús+clic y una opción nueva del menú abren la app como administrador, y
  el borde derecho de la barra superior (y el menú del dock) muestra el escritorio.
- Barra superior: indicador del idioma del teclado (ES, EN...) cuando hay más de
  uno instalado; con un clic se cambia. El reloj sigue al momento los cambios de
  hora y de zona horaria.
- Barra superior: iconos de micrófono, cámara y ubicación mientras una app los
  usa, como en la bandeja de Windows; con un clic se ve qué apps en Configuración.
- Cambiar de escritorio virtual actualiza el dock al momento: muestra las apps del
  escritorio que está delante, como la barra de Windows.
- Soporte para Windows 10 (versión 1903 o posterior). Antes el dock se cerraba
  nada más arrancar ahí. En Windows 10 el dock y la barra superior son sólidos
  en lugar de cristal, porque no ofrece desenfoque de fondo a las apps de
  escritorio.
- El botón «^» de la barra superior abre los iconos ocultos del área de
  notificación de Windows.
- Una isla curva en la barra superior muestra el uso de Claude, Codex, GitHub
  Copilot, Cursor, OpenCode Go y Gemini o Antigravity mientras están abiertos.
  Se puede desactivar en Ajustes → Barra superior.
- Ajustes tiene secciones separadas para Dock, Ventanas, Barra superior y
  Sistema; su ventana se puede redimensionar o maximizar.
- Por defecto, las ventanas minimizadas vuelan hacia el icono de su app en el
  dock. Las miniaturas en vivo a la derecha siguen disponibles como opción.
- Se quitó la opción de clic en la app activa: al hacer clic en la app que está
  delante, se minimiza, como en la barra de Windows.
- El icono de Muelle se ve más nítido en el dock y el botón Inicio usa el logo
  de Windows.
- Ajustes permite desactivar la búsqueda automática de actualizaciones. La web
  incorpora páginas de privacidad y firma de código.

## [0.1.5]

### English

- The dock appears sooner at startup (about 170 ms instead of 260-450 ms).
- Lower idle CPU: windows changing their title (terminals, browsers) no longer
  make the dock re-read every window.
- Genie starts sooner after minimizing: the screen copy no longer waits extra
  frames.
- Fixed: the dock and menu bar could stay black after energy saver; without
  transparency they are now solid gray, as macOS draws them with "Reduce
  transparency".
- Fixed: the menu bar could miss an update and showed no app name until the
  first window switch.
- Menu bar: the battery shows a charging bolt.
- Settings: options that depend on another fold away under it.

### Español

- El dock aparece antes al arrancar (unos 170 ms en lugar de 260-450 ms).
- Menos CPU en reposo: las ventanas que cambian de título (terminales,
  navegadores) ya no hacen que el dock relea todas las ventanas.
- El genio empieza antes al minimizar: la copia de pantalla ya no espera
  fotogramas de más.
- Corregido: el dock y la barra superior podían quedarse negros tras el ahorro
  de energía; sin transparencia ahora son grises sólidos, como macOS con
  «Reducir transparencia».
- Corregido: la barra superior podía perder una actualización y no mostraba el
  nombre de la app hasta el primer cambio de ventana.
- Barra superior: la batería muestra un rayo mientras carga.
- Ajustes: las opciones que dependen de otra se pliegan bajo ella.

## [0.1.4]

### English

- Maintenance release: align Genie default documentation and tests with the
  shipped setting.
- Website: add an Open Graph preview image and document the observed idle
  memory reading.

### Español

- Versión de mantenimiento: alinear la documentación y las pruebas del efecto
  Genio con el ajuste publicado.
- Web: añadir una imagen de vista previa Open Graph y documentar la lectura
  observada de memoria en reposo.

## [0.1.2]

### English

- Updates are announced in Settings: a banner above every section with
  "Update now", "Later" and "What's new", download progress, and a badge on
  System while an update waits.
- Smoother Genie effect: the system compositor now runs the whole animation
  at the display's refresh rate, and it no longer skips its first frames.
- Genie: no more flash when minimizing, and a window restored with Alt+Tab or
  from the app itself animates again the next time it is minimized.
- Fixed: the Dock section of Settings could not scroll to its last options.
- Fixed: opening Settings twice opened two windows; it now brings the open one
  to the front.
- Fixed: with an app running as administrator in front (Task Manager, an admin
  terminal, an installer), the dock's Start button and the menu bar buttons did
  nothing, because Windows drops simulated keys aimed at such apps.

### Español

- Las actualizaciones se anuncian en Ajustes: un aviso encima de cada sección
  con «Actualizar ahora», «Más tarde» y «Novedades», progreso de la descarga y
  una insignia en Sistema mientras haya una pendiente.
- Efecto genio más fluido: el compositor del sistema ejecuta toda la animación
  a la frecuencia del monitor y ya no se salta sus primeros fotogramas.
- Genio: ya no hay destello al minimizar, y una ventana restaurada con Alt+Tab o
  desde la propia app vuelve a animarse la siguiente vez que se minimiza.
- Corregido: la sección Dock de Ajustes no dejaba bajar hasta las últimas opciones.
- Corregido: abrir Ajustes dos veces abría dos ventanas; ahora trae al frente la abierta.
- Corregido: con una app de administrador delante (Administrador de tareas, una
  terminal de administrador, un instalador), el botón Inicio del dock y los botones
  de la barra superior no hacían nada, porque Windows descarta las teclas simuladas
  dirigidas a esas apps.

## [0.1.1]

### English

- Menu bar: network (Wi-Fi strength, Ethernet or offline) and volume, updated
  as they change. Search, Task View and Widgets buttons appear when they are on
  in your Windows taskbar settings.
- Clicking the app that is already in front minimizes it, as the Windows
  taskbar does; click again to bring it back. Settings can switch this to
  "do nothing", as on a Mac.
- Apps opened from the dock come to the front reliably instead of sometimes
  opening behind other windows.
- Minimized apps that run as administrator, such as Task Manager, can be brought
  back from the dock.
- Settings: restore defaults, and options to show open-app indicators, show
  recent apps and animate opening apps.

### Español

- Barra superior: red (intensidad del Wi-Fi, Ethernet o sin conexión) y volumen,
  actualizados al momento. Los botones de búsqueda, Vista de tareas y Widgets
  aparecen si los tienes activados en la barra de tareas de Windows.
- Clic en la app que ya está delante la minimiza, como la barra de Windows; otro
  clic la vuelve a traer. En Ajustes se puede cambiar a «no hacer nada», como en Mac.
- Las apps abiertas desde el dock aparecen delante siempre, en vez de abrirse a
  veces detrás de otras ventanas.
- Las apps minimizadas que se ejecutan como administrador, como el Administrador
  de tareas, se pueden recuperar desde el dock.
- Ajustes: restablecer valores y opciones para mostrar los indicadores de apps
  abiertas, mostrar las apps recientes y animar al abrir apps.

## [0.1.0]

### English

- First release: glass dock with magnification, live minimized-window
  thumbnails, optional Genie effect, menu bar, Settings app in English and
  Spanish, one-step installer and in-app updates.

### Español

- Primera versión: dock de cristal con ampliación, miniaturas en vivo de las
  ventanas minimizadas, efecto genio opcional, barra superior, app de Ajustes en
  español e inglés, instalador de un paso y actualizaciones desde la app.
