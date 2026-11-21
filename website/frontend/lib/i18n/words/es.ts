import type { Words } from "./en";

export const es: Words = {
  meta: {
    title: "Tributary: un gestor de descargas para Windows",
    description:
      "Tributary reparte cada descarga entre varias conexiones, continúa tras un fallo, captura las descargas de Chrome y Edge y guarda el vídeo de las páginas que ves. Gratis, para Windows.",
    downloadTitle: "Descargar",
    downloadDescription:
      "Descarga Tributary para Windows 10 y 11: el instalador y su suma SHA-256, cómo instalarlo, por qué avisa SmartScreen y cómo conectar Chrome o Edge.",
    downloadOgTitle: "Descarga Tributary para Windows",
    changelogTitle: "Novedades",
    changelogDescription:
      "Todas las versiones de Tributary, el gestor de descargas para Windows, de la más reciente a la más antigua, y los hitos que dieron forma a la primera.",
    changelogOgTitle: "Novedades de Tributary",
    privacyTitle: "Privacidad",
    privacyDescription:
      "Lo que guarda el sitio web de Tributary (un recuento de descargas y registros breves del servidor, sin cookies ni analítica) y lo que guarda la aplicación: todo se queda en tu ordenador.",
    privacyOgTitle: "Privacidad de Tributary",
    notFoundTitle: "Página no encontrada",
  },
  site: {
    skip: "Ir al contenido",
    main: "Principal",
    howItWorks: "Cómo funciona",
    features: "Funciones",
    footage: "Verlo en marcha",
    changelog: "Novedades",
    download: "Descargar",
    downloadForWindows: "Descargar para Windows",
    privacy: "Privacidad",
    openMenu: "Abrir el menú",
    closeMenu: "Cerrar el menú",
    language: "Idioma",
    toDark: "Cambiar al tema oscuro",
    toLight: "Cambiar al tema claro",
    copy: "Copiar",
    copied: "Copiado",
    footerAbout:
      "Un gestor de descargas gratuito para Windows que reparte cada archivo entre varias conexiones y continúa tras un fallo. Hecho con Rust, Tauri y Svelte.",
    notAffiliated: "Sin relación con ningún sitio web desde el que pueda guardar.",
  },
  hero: {
    line1: "Muchas corrientes.",
    line2: "Un archivo.",
    lede: "Tributary es un gestor de descargas gratuito para Windows que reparte cada archivo entre varias conexiones y retoma donde lo dejó tras un fallo, un apagado o una conexión caída.",
    demo: "Ver la demostración",
    version: "Versión",
    size: "Tamaño de la descarga",
    runsOn: "Funciona en",
    windows: "Windows 10 y 11, de 64 bits",
  },
  proof: {
    line: "Comprobado con {rust} pruebas de Rust, {ui} pruebas de la interfaz y la extensión y {steps} pasos de extremo a extremo, con la extensión probada en Chrome y Edge reales.",
    counted: "Contado el {date}.",
  },
  how: {
    title: "Hecho para conexiones que se caen",
    lede: "Tributary pide al servidor el archivo en hasta ocho partes y las descarga a la vez. Una conexión que termina antes se queda con la mitad de la parte más grande que sigue en curso, así que una conexión lenta no retrasa a las demás.",
    more: "Si se cae una conexión, o el ordenador falla o se apaga, continúa desde donde se detuvo. Si el archivo cambia en el servidor a mitad de la descarga, Tributary lo nota y empieza de nuevo, en lugar de juntar partes de dos archivos distintos.",
    alt: "La ventana de descarga de Tributary: descargando, 58,6 %, 762 MB de 1,3 GB a 3,2 MB/s, quedan 2 minutos y 47 segundos, reanudación compatible, y una barra de Conexiones con ocho partes llenas separadas por huecos.",
    caption: "Así es de verdad: un vídeo de 1,3 GB por ocho conexiones, en la ventana de descarga de Tributary.",
  },
  sim: {
    server: "Servidor",
    computer: "Tu ordenador",
    intro: "<b>Simulación.</b> Funciona en tu navegador y no descarga nada.",
    progress: "{got} de {size} MB, {percent} %",
    complete: "Completa",
    changed: "Cambió en el servidor",
    label: "Descarga simulada de {name}",
    own: "Descargando su propia parte",
    tookOver: "Relevó a una conexión más lenta",
    reconnecting: "Reconectando",
    reconnectingOne: "Conexión {n} reconectando",
    play: "Reproducir",
    pause: "Pausa",
    playLabel: "Reproducir la simulación",
    pauseLabel: "Pausar la simulación",
    drop: "Cortar una conexión",
    change: "Cambiar el archivo en el servidor",
    said: {
      ready: "En pausa. El archivo se divide en ocho partes, cada una descargada por su propia conexión.",
      underWay: "Descargando por ocho conexiones, cada una con su propia parte del archivo.",
      back: "La conexión {n} ha vuelto y ha seguido desde donde se detuvo.",
      tookOver: "La conexión {n} terminó su parte y se quedó con la mitad de lo que le quedaba a la conexión {m}.",
      complete: "Descarga completa. Llegaron todas las partes, unidas en un solo archivo.",
      changed:
        "El archivo cambió en el servidor. Tributary lo notó y empieza de nuevo en lugar de juntar partes de dos archivos distintos.",
      dropped: "Se cayó la conexión {n}. Tributary la vuelve a conectar y sigue desde donde se detuvo.",
      restart: "Empezando de nuevo desde el principio, con la nueva versión del archivo.",
      again: "Descargándolo otra vez, por ocho conexiones.",
    },
  },
  features: {
    title: "Descargas de dondequiera que las encuentres",
    lede: "Desde el navegador, desde el vídeo que estás viendo, desde una página de fotos o un sitio web entero. Todo se une en una lista y se descarga cuando y tan rápido como digas.",
    takeoverTitle: "Captura las descargas de Chrome y Edge",
    takeover: "Empieza una descarga en Chrome o Edge y Tributary se encarga de ella, para los tipos de archivo que elijas. Su extensión funciona en los dos navegadores.",
    takeoverKeys:
      "Mantén <key>Ctrl</key> al hacer clic para enviar una descarga a Tributary, o <key>Alt</key> para dejársela al navegador. Puedes cambiar las dos teclas.",
    takeoverMenu: "Haz clic derecho en un enlace, una selección o una página para enviar todos sus enlaces.",
    takeoverClipboard: "Puede vigilar el portapapeles por si copias enlaces, y acepta los que arrastres a su ventana.",
    takeoverAlt:
      "El cuadro Descargar archivo de Tributary para node-v22.11.0.tar.gz de nodejs.org: 92,4 MB, reanudación compatible, con nombre, carpeta, categoría y cola a elegir, y los botones Cancelar, Descargar más tarde y Empezar la descarga.",
    takeoverCaption: "Una descarga pulsada en Chrome, capturada por Tributary.",
    videoTitle: "Guarda el vídeo de la página que ves",
    video: "Aparece un botón junto a los reproductores de vídeo. Tributary guarda por sí mismo las emisiones HLS y DASH y une imagen y sonido en un solo archivo con ffmpeg, en calidades de hasta 4K a 60 fps cuando el sitio las ofrece.",
    videoScript:
      "YouTube y sitios parecidos alimentan su reproductor por script, así que no hay ningún archivo a la vista. Para ellos, Tributary pasa la página a yt-dlp, una herramienta gratuita que puede instalarte, comprobada con su suma publicada.",
    videoNote:
      "Guardar vídeo puede ir contra las condiciones de un sitio, y las de YouTube lo prohíben, así que la decisión es tuya. Los vídeos protegidos (con DRM o cifrados), en directo, de pago o que requieren iniciar sesión se rechazan, y yt-dlp nunca recibe tus cookies.",
    videoAlt:
      "El cuadro Guardar vídeo de Tributary para un vídeo de YouTube, con calidades desde 2160p60 de unos 1,3 GB hasta 144p, y la nota de que guardar vídeo puede ir contra las condiciones de un sitio y de que se rechazan los vídeos protegidos, de pago o que requieren iniciar sesión.",
    videoCaption: "Eligiendo una calidad. El cuadro dice lo mismo sobre las condiciones.",
    picturesTitle: "Guarda todas las imágenes de una página",
    pictures:
      "En una página llena de fotos o publicaciones, elige <b>Guardar todas las imágenes y vídeos de esta página</b>. Tributary recorre la página como lo harías tú, toma cada imagen en el tamaño más grande que ofrece la página y se detiene al final o ante una pantalla de inicio de sesión.",
    picturesKeep:
      "Después eliges qué conservar, y las guarda por una cola, cuatro a la vez, en una carpeta con el nombre de la página.",
    picturesNote:
      "Las imágenes pertenecen a quienes las publicaron, y las condiciones de algunos sitios, entre ellos Instagram y Facebook, prohíben recopilarlas de forma automática. Tributary nunca se salta una pantalla de inicio de sesión, una cuenta privada ni los límites de un sitio.",
    grabberTitle: "Explora un sitio por ti",
    grabber:
      "El capturador de sitios lee un sitio web de página en página, siempre respetando su robots.txt, y reúne las páginas y archivos que le pidas. Elige <b>Sitio completo</b> para guardarlo y leerlo sin conexión, con sus enlaces apuntando a las copias guardadas.",
    grabberProjects:
      "Un proyecto puede detenerse y continuarse más tarde, programarse o repetirse para guardar lo nuevo mientras la aplicación está abierta.",
    queuesTitle: "Colas, horarios, límites",
    queues: [
      ["Colas", "Varias, cada una con tantos archivos a la vez como elijas."],
      ["Horarios", "Horas de inicio y de parada, y el ordenador activado para un inicio programado."],
      ["Al terminar una cola", "Apagar el ordenador o ejecutar un programa."],
      ["Límite de velocidad", "Un tope a la velocidad de descarga de Tributary."],
      ["Cuotas", "Tanto por periodo. Las descargas se pausan al agotarse y continúan solas."],
      ["Categorías", "La lista ordenada por tipo: comprimidos, documentos, música, programas y vídeo."],
    ],
    loginsTitle: "Tus inicios de sesión fuera del disco",
    logins:
      "Tributary descarga por HTTP, HTTPS y FTP, inicia sesión en los servidores que lo piden y funciona a través de un proxy. Las contraseñas se guardan en el Administrador de credenciales de Windows, nunca en un archivo, y las cookies de tu navegador solo se guardan en memoria.",
    loginsMore:
      "Por eso, tras un reinicio, una descarga tomada del navegador puede necesitar de nuevo su enlace. No hay cuenta ni telemetría y, si quieres, cada descarga terminada se analiza en busca de virus.",
  },
  footage: {
    title: "Míralo en una descarga real",
    lede: "Sesenta y cuatro segundos grabados en la aplicación: una descarga capturada de Chrome, las conexiones llenándose, una calidad de vídeo elegida y la lista terminada.",
    fallback:
      "Tu navegador no puede reproducir este vídeo. Muestra a Tributary capturando una descarga de Chrome y guardando un vídeo.",
    caption:
      "La demostración no tiene sonido. Guarda Big Buck Bunny, un cortometraje que se puede copiar libremente. Big Buck Bunny (c) Blender Foundation, CC BY 3.0.",
  },
  commandLine: {
    title: "Contrólalo desde un script",
    lede: "Añade descargas, inicia la cola o abre la aplicación en la bandeja desde una terminal, un acceso directo o una tarea programada. Si Tributary ya está abierto, la orden va a la copia en marcha.",
    label: "Línea de órdenes",
    copy: "Copiar la sintaxis de la línea de órdenes",
    options: {
      add: "Añade una descarga. Sin --silent, se te pregunta antes.",
      folder: "Dónde guardarla.",
      name: "Cómo llamarla.",
      silent: "Empieza sin preguntas. Una emisión de vídeo se guarda en su mejor calidad.",
      later: "La pone en la cola principal en lugar de empezarla.",
      startQueue: "Inicia la cola principal.",
      minimized: "Se abre en la bandeja.",
      quitWhenDone:
        "Cierra la aplicación al terminar esta descarga. Necesita --add y --silent. La aplicación sigue abierta si la descarga falla, si hay otra en curso o si el análisis de virus está activado.",
    },
  },
  faq: {
    title: "Preguntas frecuentes",
    items: [
      { id: "free", q: "¿Tributary es gratis?", a: "Sí. No tiene precio, ni cuenta, ni publicidad." },
      {
        id: "safe",
        q: "¿Es seguro ejecutar el instalador?",
        a: "El instalador no está firmado, así que Windows SmartScreen avisa de él. Comprueba que el SHA-256 de tu descarga coincide con el de esta página y elige <b>Más información</b> y <b>Ejecutar de todas formas</b>. Tributary no tiene cuenta ni telemetría, guarda las contraseñas de los servidores en el Administrador de credenciales de Windows y las cookies del navegador solo en memoria. También puede analizar cada descarga terminada en busca de virus, si lo activas.",
      },
      {
        id: "youtube",
        q: "¿Descarga vídeos de YouTube?",
        a: "Sí, mediante yt-dlp, una herramienta gratuita que Tributary puede instalarte y comprobar con su suma publicada. Guardar vídeo puede ir contra las condiciones de un sitio, y las de YouTube lo prohíben, así que esa decisión es tuya. Los vídeos protegidos (con DRM o cifrados), en directo, de pago o que requieren iniciar sesión se rechazan, y yt-dlp nunca recibe tus cookies.",
      },
      {
        id: "privacy",
        q: "¿Qué sabe de mí?",
        a: "No hay cuenta ni telemetría. Las contraseñas de los servidores se guardan en el Administrador de credenciales de Windows, nunca en el disco, y las cookies de tu navegador solo en memoria.",
      },
      {
        id: "browsers",
        q: "¿Con qué navegadores funciona?",
        a: "Con Chrome y Edge, mediante una extensión que añades tú, porque los navegadores no dejan que otros programas instalen extensiones. Los pasos están en la sección de descarga, y completos en la página de descarga.",
      },
      {
        id: "sites",
        q: "¿Puede guardar un sitio web entero?",
        a: "Sí. El capturador de sitios lee un sitio de página en página y siempre respeta su <code>robots.txt</code>. Elige <b>Sitio completo</b> para guardar las páginas y leerlas sin conexión.",
      },
      {
        id: "update",
        q: "He actualizado Tributary. ¿Por qué falta una función nueva?",
        a: "Recarga la extensión. Abre <code>chrome://extensions</code> (o <code>edge://extensions</code>) y pulsa la flecha circular de la tarjeta de Tributary. El navegador suele seguir en segundo plano y conservar la extensión antigua.",
      },
    ],
  },
  panel: {
    title: "Descarga Tributary {version}",
    lede: "Gratis, para Windows 10 y 11. Un instalador, sin cuenta.",
    version: "Versión",
    size: "Tamaño",
    published: "Publicada",
    file: "Archivo",
    checksum: "Suma SHA-256",
    copyChecksum: "Copiar la suma SHA-256",
    noChecksum:
      "No se cargaron los datos de la versión, así que no se puede mostrar la suma. Recarga la página dentro de un minuto para verla antes de ejecutar el instalador.",
    need: "Qué necesitas",
    install: "Instálalo",
    steps: [
      ["Descarga el instalador", "Guárdalo donde quieras. Si quieres, comprueba su SHA-256 con el de arriba."],
      [
        "Ábrelo y supera SmartScreen",
        "El instalador no está firmado, así que Windows avisa de él. Elige Más información y después Ejecutar de todas formas.",
      ],
      [
        "Deja que se instale",
        "Se instala solo para tu usuario y no necesita permisos de administrador. Si falta Microsoft Edge WebView2, lo descarga.",
      ],
      [
        "Añade la extensión del navegador",
        "Chrome y Edge no dejan que otros programas instalen extensiones, así que añades tú la de Tributary. La página de descarga tiene todos los pasos.",
      ],
    ],
    guide: "Leer la guía de instalación completa",
  },
  facts: {
    requirements: [
      "Windows 10 u 11, de 64 bits",
      "Se instala solo para tu usuario, así que no necesita permisos de administrador",
      "Microsoft Edge WebView2, que viene con Windows 11; el instalador lo descarga si falta",
    ],
    history: [
      {
        title: "El motor de descargas",
        detail:
          "Descargas repartidas entre varias conexiones, con reanudación, y el servidor de pruebas que corta conexiones y cambia archivos a mitad de descarga para demostrarlo. La aplicación de escritorio y su instalador.",
      },
      {
        title: "Todo lo que rodea a las descargas",
        detail:
          "Colas, un programador, límites de velocidad y categorías. FTP, inicios de sesión en servidores y proxies. La captura de descargas del navegador. Guardar vídeo HLS y DASH. El capturador de sitios. Contraseñas fuera del disco. Probado en Chrome real.",
      },
      {
        title: "Vídeo de la página que estás viendo",
        detail:
          "Un botón de descarga junto a cualquier vídeo, YouTube incluido. El vídeo correcto aunque se abriera desde una lista, y calidades con los nombres que usan los sitios de vídeo.",
      },
      {
        title: "Más sitios, y un archivo por vídeo",
        detail:
          "Reproductores insertados desde otros sitios, y sitios que solo ofrecen HLS. Imagen y sonido llegan como un solo elemento y un solo archivo.",
      },
      {
        title: "Todas las imágenes y vídeos de una página",
        detail:
          "Tributary recorre una página como lo harías tú, reúne lo que muestra en el tamaño más grande disponible y lo guarda todo con un clic.",
      },
    ],
  },
  download: {
    title: "Descarga Tributary",
    lede: "Gratis, para Windows 10 y 11. Se instala solo para tu usuario, así que no necesita permisos de administrador.",
    onThisPage: "En esta página",
    sections: {
      requirements: "Qué necesitas",
      check: "Comprueba la descarga",
      install: "Instalar",
      smartscreen: "Si Windows te avisa",
      browser: "Conectar Chrome o Edge",
      update: "Actualizar",
      uninstall: "Desinstalar",
    },
    checkIntro: "Una suma de comprobación demuestra que tu archivo es el publicado aquí, sin cambios por el camino.",
    checkRun:
      "Abre PowerShell y ejecuta estas dos líneas. Si guardaste el instalador en otra carpeta que no sea Descargas, cambia la carpeta en la primera línea.",
    copyCommands: "Copiar las órdenes de PowerShell",
    checkHash:
      "El <b>Hash</b> que muestra debe coincidir con la suma SHA-256 de esta página. PowerShell lo escribe en mayúsculas, lo que no cambia nada.",
    checkCompare: "O deja que PowerShell las compare por ti. Escribe <b>True</b> cuando coinciden.",
    copyComparison: "Copiar la comparación de PowerShell",
    checkMismatch: "Si no coinciden, no ejecutes el archivo. Elimínalo y descárgalo de nuevo.",
    installSteps: {
      downloadTitle: "Descarga el instalador",
      download:
        "Elige <b>Descargar para Windows</b>. Tu navegador guarda <code>{file}</code>, normalmente en tu carpeta Descargas.",
      checkLink: "Comprueba que llegó intacto",
      runTitle: "Ejecútalo",
      run: "Abre el archivo. Probablemente Windows SmartScreen te avise, porque el instalador no está firmado: elige <b>Más información</b> y después <b>Ejecutar de todas formas</b>.",
      whyLink: "Por qué avisa Windows",
      followTitle: "Sigue el instalador",
      follow:
        "Tributary se instala solo para tu cuenta de usuario, así que no pide permisos de administrador. Necesita Microsoft Edge WebView2, que viene con Windows 11; si falta, el instalador lo descarga.",
      openTitle: "Abre Tributary",
      open: "Ábrelo desde el menú Inicio.",
      connectLink: "Deja que capture las descargas de tu navegador",
    },
    smartscreenIntro: "Por qué SmartScreen detiene el instalador y cómo seguir.",
    smartscreen:
      "Windows SmartScreen avisa de los programas que aún no conoce. El instalador de Tributary no está firmado con un certificado de firma de código, así que Windows no puede confirmar quién lo publicó y muestra <b>Windows protegió su PC</b>.",
    smartscreenCheck: "Si coincide, elige <b>Más información</b> y después <b>Ejecutar de todas formas</b>.",
    smartscreenCheckLink: "Primero, comprueba la suma SHA-256 del archivo.",
    smartscreenMismatch: "Si no coincide, no lo ejecutes. Elimina el archivo y descárgalo de nuevo.",
    browserIntro:
      "Una vez por navegador, para que Tributary pueda capturar descargas y guardar el vídeo de las páginas que ves.",
    browserWhy: "Chrome y Edge no dejan que otros programas instalen extensiones, así que añades tú la de Tributary.",
    browserSteps: {
      copyTitle: "Copia la carpeta de la extensión",
      copy: "En Tributary, abre <b>Opciones</b>, después la pestaña <b>Navegador</b>, y copia la carpeta que aparece.",
      developerTitle: "Activa el modo de desarrollador",
      developer:
        "Abre <code>chrome://extensions</code> en Chrome, o <code>edge://extensions</code> en Edge, y activa el <b>Modo de desarrollador</b>.",
      loadTitle: "Carga la extensión",
      load: "Elige <b>Cargar descomprimida</b> y selecciona la carpeta que copiaste.",
    },
    browserOthers: "Desactiva la extensión de cualquier otro gestor de descargas. Las dos intentarían quedarse con las mismas descargas.",
    browserKeys:
      "Mantén <key>Ctrl</key> al hacer clic en un enlace para enviarlo a Tributary, o <key>Alt</key> para dejárselo al navegador. Puedes cambiar las dos teclas en <b>Opciones</b>.",
    updateIntro: "Tributary no se actualiza solo. Las versiones nuevas aparecen en las novedades.",
    changelogLink: "Ver las novedades",
    updateSteps: {
      installTitle: "Instala la versión nueva",
      install: "Descárgala desde esta página y ejecuta el instalador sobre la versión que tienes.",
      reloadTitle: "Recarga la extensión",
      reload:
        "Abre <code>chrome://extensions</code> (o <code>edge://extensions</code>) y pulsa la flecha circular de la tarjeta de Tributary. Chrome suele seguir en segundo plano tras cerrar sus ventanas y conservar la extensión antigua, así que las funciones nuevas no aparecen hasta que la recargas.",
    },
    uninstall:
      "Abre la <b>Configuración</b> de Windows y después <b>Aplicaciones</b>. Busca Tributary entre tus aplicaciones instaladas y elige <b>Desinstalar</b>. Para quitar también tu lista de descargas, colas y ajustes, marca <b>Eliminar los datos de la aplicación</b> en el desinstalador.",
    uninstallNotes: [
      "Los archivos que descargaste se quedan donde los guardaste.",
      "El desinstalador desconecta la extensión del navegador. Quítala también del navegador, en <code>chrome://extensions</code> o <code>edge://extensions</code>.",
      "Las contraseñas que guardaste para sitios se quedan en el Administrador de credenciales de Windows, con nombres que empiezan por <code>Tributary/</code>, hasta que las quites allí.",
    ],
  },
  release: {
    forWindows: "Para Windows 10 y 11, de 64 bits",
    howToCheck: "Cómo comprobar tu descarga",
  },
  changelog: {
    title: "Novedades",
    lede: "Lo que trajo cada versión de Tributary, de la más reciente a la más antigua, y cómo se hizo la primera.",
    version: "Versión {version}",
    latest: "Última versión",
    noNotes:
      "No se cargaron las notas de esta versión. Recarga la página dentro de un minuto para verlas; el historial de abajo muestra lo que incluye.",
    downloadSized: "Descargar {version} ({size})",
    downloadPlain: "Descargar {version}",
    checkChecksum: "Comprobar su suma",
    builtTitle: "Cómo se hizo la {version}",
    builtIntro: "Hitos del historial de cambios del proyecto, del más antiguo al más reciente.",
    released: "Publicada como versión {version}",
    releasedOn: "Publicada como versión {version} el {date}",
    getIt: "Consíguela, con su suma y los pasos de instalación",
    notes: {
      "0.1.0": [
        "Reparte cada descarga entre varias conexiones y continúa tras un fallo, un apagado o una conexión caída.",
        "Captura las descargas de Chrome y Edge, con un menú contextual para enlaces y un botón de descarga junto a los reproductores de vídeo.",
        "Guarda vídeo, incluidos HLS, DASH y YouTube mediante yt-dlp, y une imagen y sonido en un solo archivo.",
        "Guarda todas las imágenes y vídeos de una página y explora sitios web enteros con el capturador de sitios.",
        "Colas, un programador, un limitador de velocidad y cuotas, FTP, inicios de sesión en servidores y proxies.",
        "Guarda las contraseñas en el Administrador de credenciales de Windows. Sin cuenta y sin telemetría.",
      ],
    },
  },
  privacy: {
    title: "Privacidad",
    lede: "Este sitio web cuenta las descargas y guarda registros breves del servidor. La aplicación lo guarda todo en tu ordenador, sin cuenta y sin telemetría.",
    updated: "Última actualización: {date}",
    websiteTitle: "Lo que guarda este sitio web",
    websiteIntro: "Lo mínimo posible, y solo el tiempo que hace falta.",
    howLong: "Cuánto tiempo",
    kept: [
      {
        what: "Recuento de descargas",
        detail:
          "Cada descarga del instalador se registra con su versión, la fecha y la hora, y si tu navegador dijo que funciona en Windows. No se registra nada más con ella.",
        howLong: "Mientras el sitio funcione, como el número de descargas de cada versión",
      },
      {
        what: "Un código que evita contar dos veces",
        detail:
          "Para que una persona que descarga dos veces en un día cuente una sola vez, el sitio crea un código de un solo sentido (un hash con sal) a partir de tu dirección IP, el agente de usuario de tu navegador y la fecha. Tu dirección IP no se guarda.",
        howLong: "Un día; después se elimina",
      },
      {
        what: "Registros del servidor",
        detail:
          "Por cada solicitud, nginx, el servidor web que está delante del sitio, registra la hora, tu dirección IP, la dirección que pediste, el código y el tamaño de la respuesta, y cuánto tardó. No registra qué navegador usas ni de qué página vienes.",
        howLong:
          "Hasta tres archivos de 10 MB; cuando se llenan, se elimina el más antiguo, así que el tiempo depende de cuánto se use el sitio",
      },
      {
        what: "Tu tema y tu idioma",
        detail:
          "Si cambias entre el tema claro y el oscuro, tu navegador recuerda la elección en tu dispositivo. Nunca se envía al sitio. El idioma va en la dirección de la página.",
        howLong: "Hasta que borres los datos de este sitio en tu navegador",
      },
    ],
    notDoneTitle: "Lo que no hace",
    notDone: [
      "No usa cookies.",
      "No ejecuta scripts de analítica, seguimiento ni publicidad.",
      "No carga nada de otras empresas: las fuentes, las imágenes y el vídeo vienen todos de este sitio.",
    ],
    appTitle: "Lo que guarda la aplicación",
    appIntro:
      "Tributary guarda lo que sabe en tu ordenador. No hay cuenta ni telemetría: no informa de lo que descargas ni de cómo lo usas.",
    app: [
      "Tu lista de descargas, colas y ajustes están en <code>%APPDATA%\\app.tributary.desktop</code>.",
      "Las contraseñas de sitios y servidores se guardan en el Administrador de credenciales de Windows, nunca en un archivo.",
      "Las cookies que tu navegador pasa para una descarga solo se guardan en memoria y nunca se escriben en el disco, así que, tras un reinicio, esa descarga puede necesitar de nuevo su enlace.",
      "La extensión del navegador solo habla con Tributary en tu ordenador.",
      "Tributary se conecta a los sitios desde los que descargas y, cuando le pides que instale o actualice yt-dlp, al lugar donde se publica yt-dlp. yt-dlp nunca recibe tus cookies ni tus inicios de sesión.",
      "Una vez al día, Tributary pregunta a este sitio si hay una versión nueva. Solo envía su versión y que funciona en Windows de 64 bits, y puedes desactivarlo en Opciones, General.",
      "Cuando pulsas Buscar en su cuadro de inicio, Tributary pide al sitio desde el que descargas la suma que publica junto al archivo, sin tus cookies.",
      "Las réplicas y los servidores Metalink que tú o un sitio indiquéis reciben las partes del archivo que sirven. Tus cookies e inicios de sesión solo van al sitio del propio archivo.",
    ],
  },
  notFound: {
    title: "No hay ninguna página en esta dirección.",
    lede: "Puede que el enlace esté anticuado o que la dirección esté mal escrita. Todo lo que hay en este sitio está a un clic.",
    home: "Ir a la página de inicio",
    download: "Descargar Tributary",
    more: "Más páginas",
  },
};
