<div align="center">

<img width="50%" alt="Muelle" src="https://github.com/user-attachments/assets/8c715a53-a0df-4780-9dd4-09ce3ccd9043" />

  **Un dock ligero, inspirado en macOS, para Windows 11.**

  Ampliación suave, miniaturas de ventanas en vivo y un efecto genio opcional al minimizar.

  [English](README.md) · **Español**
</div>

Muelle sustituye la barra de tareas de Windows por un dock personalizable y una
barra superior opcional al estilo de macOS. Está hecho para Windows 11 y sigue
en desarrollo activo.

<img width="1919" height="1008" alt="Muelle en Windows 11" src="https://github.com/user-attachments/assets/b6c44203-c425-43fb-adf5-e2edd2f02257" />

## Funciones

- Dock de cristal con ampliación suave de los iconos bajo el cursor.
- Abrir, fijar, reordenar y gestionar apps, incluidas las recientes y las ventanas abiertas.
- Miniaturas en vivo de las ventanas minimizadas.
- Clic en la app que ya está delante para minimizarla, como en la barra de Windows.
- Arrastrar y soltar archivos sobre apps, Descargas o la Papelera.
- Ocultación automática opcional.
- Barra superior con la app en primer plano, hora, red, volumen y batería, además de
  los botones de búsqueda, Vista de tareas y Widgets si los tienes en la barra de Windows.
- App de Ajustes aparte, con cambios en vivo, en español e inglés, y efecto genio configurable.
- Un proceso guardián devuelve la barra de tareas de Windows si el dock se cierra de forma inesperada.

El efecto genio está **desactivado por defecto**. Mientras está activo, Muelle
desactiva las transiciones de Windows en las ventanas que sigue, para poder
dibujar su animación; las devuelve al cerrarse.

## Instalar

**[Descargar Muelle-Setup.exe](https://github.com/crafter-station/Muelle/releases/latest/download/Muelle-Setup.exe)**
(Windows 11, 64 bits) o elige una versión en
[Releases](https://github.com/crafter-station/Muelle/releases).

1. Ejecuta `Muelle-Setup.exe`. Se instala solo para tu usuario: sin permisos de
   administrador, sin asistente, en unos dos segundos.
2. El instalador aún no está firmado, así que SmartScreen puede decir que protegió
   tu PC. Elige **Más información → Ejecutar de todas formas**.
3. Aparece el dock y se oculta la barra de tareas de Windows. Desde entonces Muelle
   se abre al iniciar Windows; puedes desactivarlo en Ajustes → Sistema.

Abre **Muelle** desde el menú Inicio (o clic derecho en el dock → *Ajustes de
Muelle…*) para cambiar el dock. Ajustes también busca actualizaciones y las instala.

**Recuperar la barra de tareas:** clic derecho en el dock → *Salir de Muelle*.
Para quitarlo del todo: Configuración de Windows → Aplicaciones → Aplicaciones
instaladas → Muelle → Desinstalar; la barra vuelve y no queda nada en marcha.

## Probarlo desde el código

### Requisitos

- Windows 11
- Rust con la toolchain MSVC y las Build Tools de C++ de Visual Studio
- Node.js y pnpm

### Ejecutar el dock

Desde la raíz del repositorio:

```powershell
cargo run -p muelle
```

El dock oculta la barra de tareas mientras está en marcha. Pulsa **Ctrl+C** en la
terminal para salir; Muelle debería devolver la barra solo. Para devolverla a mano:

```powershell
cargo run -p muelle -- --restore
```

### Ejecutar Ajustes

Con el dock en marcha, abre otra terminal y ejecuta:

```powershell
cd apps\settings
pnpm install
pnpm tauri dev
```

Se abre la ventana de Ajustes. Los cambios de la interfaz se recargan solos; tras
cambiar el backend en Rust, reinicia `tauri dev`. Los cambios de Ajustes se
aplican al dock en marcha.

## Compilar y probar

Desde la raíz del repositorio:

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
```

Para compilar la interfaz de Ajustes:

```powershell
cd apps\settings
pnpm build
```

## Estructura

| Ruta | Para qué |
|---|---|
| `apps/dock` | El dock nativo de Windows |
| `apps/settings` | La app de Ajustes (Tauri) |
| `crates/muelle-core` | Lógica de disposición y animación, independiente de la plataforma |
| `crates/muelle-shell` | Integración con el shell de Windows |
| `crates/muelle-config` | Ajustes compartidos y su guardado |
| `muelle/` | La web (Astro) |

## Compilar el instalador

```powershell
cd apps\settings
pnpm install
cd ..\..
.\scripts\build-installer.ps1
```

El script compila las dos apps y genera `Muelle_<versión>_x64-setup.exe` en
`target\release\bundle\nsis`. El instalador es por usuario, no pide permisos de
administrador ni muestra asistente: instala en `%LOCALAPPDATA%\Programs\Muelle`,
activa el inicio con Windows y abre el dock. Al desinstalar (Configuración →
Aplicaciones) se cierra el dock y vuelve la barra de tareas de Windows.

Al subir una etiqueta que coincida con la versión (por ejemplo `v0.1.1`), GitHub
Actions compila el instalador y lo adjunta a un borrador de release, junto con
`Muelle-Setup.exe` (el enlace de descarga de la web), `latest.json` (lo lee el
actualizador de Ajustes) y las notas de esa versión sacadas de `CHANGELOG.md`.
El repositorio necesita los secretos `TAURI_SIGNING_PRIVATE_KEY` y
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Publica el borrador para que pase a ser la
descarga y la actualización.

## Estado

El dock, Ajustes y el instalador funcionan en Windows 11. Algunas funciones e
interacciones aún se están verificando. El instalador todavía no está firmado,
así que SmartScreen avisa antes de ejecutarlo (Más información → Ejecutar de
todas formas).
