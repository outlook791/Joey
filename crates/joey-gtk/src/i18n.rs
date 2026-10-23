//! Interface text in the system language. English is the source; Italian
//! is built in. More languages can move to gettext later without touching
//! the call sites: `t("…")` and `tf("… {n} …", &[("n", …)])`.

use std::sync::OnceLock;

fn italian() -> bool {
    static IT: OnceLock<bool> = OnceLock::new();
    *IT.get_or_init(|| {
        for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if !v.is_empty() {
                    return v.starts_with("it");
                }
            }
        }
        false
    })
}

const IT: &[(&str, &str)] = &[
    // Pocket
    ("Drop files here", "Trascina qui i file"),
    ("Choose files…", "Scegli file…"),
    ("Choose files to convert", "Scegli i file da convertire"),
    ("Esc to close", "Esc per chiudere"),
    ("1 file", "1 file"),
    ("{n} files", "{n} file"),
    ("{n} {fmt} files", "{n} file {fmt}"),
    ("{n} more…", "Altri {n}…"),
    ("Quick tools", "Strumenti rapidi"),
    ("Convert", "Converti"),
    ("Adjust", "Regola"),
    ("Enter", "Invio"),
    ("PDF · one image per page", "PDF · un'immagine per pagina"),
    // Kinds
    ("image", "immagine"),
    ("drawing", "disegno"),
    ("audio", "audio"),
    ("video", "video"),
    ("PDF", "PDF"),
    ("document", "documento"),
    ("spreadsheet", "foglio di calcolo"),
    ("presentation", "presentazione"),
    // Skipped files
    ("folders aren't supported yet", "le cartelle non sono ancora supportate"),
    ("not found", "non trovato"),
    ("file type not recognised", "tipo di file non riconosciuto"),
    ("nothing to convert {fmt} to here", "nessuna conversione possibile per {fmt} su questo computer"),
    // Tools
    ("Compress", "Comprimi"),
    ("Remove metadata", "Rimuovi metadati"),
    ("Merge into one PDF", "Unisci in un solo PDF"),
    ("Join into one track", "Unisci in un'unica traccia"),
    ("Even out the volume", "Uniforma il volume"),
    ("Remove the sound", "Togli l'audio"),
    // Progress and result
    ("Converting…", "Conversione…"),
    ("Converting {name}…", "Conversione di {name}…"),
    ("Converting {i} of {n}…", "Conversione {i} di {n}…"),
    ("Nothing converted", "Nessun file convertito"),
    ("1 file ready", "1 file pronto"),
    ("{n} files ready", "{n} file pronti"),
    ("Next to the originals", "Accanto agli originali"),
    ("Show in folder", "Mostra nella cartella"),
    ("Close", "Chiudi"),
    ("Back to the menu", "Torna al menu"),
    ("Other files", "Altri file"),
    ("unexpected error", "errore imprevisto"),
    ("{tool} couldn't convert it", "{tool} non è riuscito a convertirlo"),
    ("already as small as it gets", "è già al minimo, non si riduce di più"),
    ("the file has no audio track", "il file non contiene audio"),
    // Adjust screen
    ("Back", "Indietro"),
    ("Reset to defaults", "Ripristina predefiniti"),
    ("Remember these settings", "Ricorda queste impostazioni"),
    ("Advanced", "Avanzate"),
    ("No settings for this one.", "Nessuna impostazione per questa scelta."),
    ("Estimated size: {size}", "Peso stimato: {size}"),
    ("Size can't be estimated for this conversion", "Il peso non si può stimare per questa conversione"),
    ("Quality", "Qualità"),
    ("Lower quality makes smaller files", "Qualità più bassa = file più leggero"),
    ("Low", "Bassa"),
    ("Medium", "Media"),
    ("High (recommended)", "Alta (consigliata)"),
    ("Maximum", "Massima"),
    ("Size", "Dimensioni"),
    ("Longest side. Never enlarges.", "Lato più lungo. Non ingrandisce mai."),
    ("Original", "Originale"),
    ("Very large · 3840 px", "Molto grande · 3840 px"),
    ("Large · 1920 px", "Grande · 1920 px"),
    ("Medium · 1280 px", "Media · 1280 px"),
    ("Small · 800 px", "Piccola · 800 px"),
    ("Remove camera and location info", "Togli dati di fotocamera e posizione"),
    ("Date, place and camera model saved inside the photo", "Data, luogo e modello di fotocamera salvati nella foto"),
    ("Channels", "Canali"),
    ("Mono halves the size; fine for voice", "Mono dimezza il peso; va bene per la voce"),
    ("Mono", "Mono"),
    ("Stereo", "Stereo"),
    ("Bitrate", "Bitrate"),
    ("Data per second: higher means better sound and a bigger file. Replaces Quality.", "Dati al secondo: più alto = suono migliore e file più pesante. Sostituisce Qualità."),
    ("Automatic", "Automatico"),
    ("Sample rate", "Frequenza di campionamento"),
    ("How often the sound is measured each second. Change it only if a device asks.", "Quante volte al secondo viene misurato il suono. Cambiala solo se un dispositivo lo richiede."),
    ("44.1 kHz · CD", "44,1 kHz · CD"),
    ("48 kHz · video", "48 kHz · video"),
    ("Remove song info", "Togli le informazioni del brano"),
    ("Title, artist, album and similar tags", "Titolo, artista, album e simili"),
    ("Resolution", "Risoluzione"),
    ("Smaller resolution, much smaller file", "Risoluzione più bassa, file molto più leggero"),
    ("4K · 2160p", "4K · 2160p"),
    ("Full HD · 1080p", "Full HD · 1080p"),
    ("HD · 720p", "HD · 720p"),
    ("Small · 480p", "Piccola · 480p"),
    ("Keep the sound", "Mantieni l'audio"),
    ("Frames per second", "Fotogrammi al secondo"),
    ("How smooth the motion is. 30 is fine for most videos.", "Quanto è fluido il movimento. 30 va bene per quasi tutti i video."),
    ("Remove video info", "Togli le informazioni del video"),
    ("Date, place and device saved inside the video", "Data, luogo e dispositivo salvati nel video"),
    ("Width", "Larghezza"),
    ("Small · 320 px", "Piccola · 320 px"),
    ("Medium · 480 px", "Media · 480 px"),
    ("Large · 720 px", "Grande · 720 px"),
    ("Pages", "Pagine"),
    ("For example 1-3, 7. Empty means all.", "Ad esempio 1-3, 7. Vuoto = tutte."),
    ("all", "tutte"),
    ("Sharpness", "Nitidezza"),
    ("Dots per inch: higher is sharper and bigger", "Punti per pollice: più alto = più nitido e più pesante"),
    ("Screen · 72 dpi", "Schermo · 72 dpi"),
    ("Normal · 150 dpi", "Normale · 150 dpi"),
    ("Print · 300 dpi", "Stampa · 300 dpi"),
    ("Very sharp · 600 dpi", "Molto nitida · 600 dpi"),
    // File manager integration
    ("Set up Alt+C in your file manager", "Attiva Alt+C nel file manager"),
    ("Select files, press Alt+C, pick a format", "Seleziona i file, premi Alt+C, scegli il formato"),
    ("Alt+C is ready in {fm}", "Alt+C è pronto in {fm}"),
    ("Right-click → Convert with Joey works in {fm}", "Tasto destro → Converti con Joey funziona in {fm}"),
    ("Restart the file manager once to use it", "Riavvia il file manager una volta per usarlo"),
    ("No supported file manager found", "Nessun file manager supportato trovato"),
    ("Convert with Joey", "Converti con Joey"),
    // Tangerine-parity additions
    ("subtitles", "sottotitoli"),
    ("archive", "archivio"),
    ("other", "altro"),
    ("{n} other items", "{n} altri elementi"),
    ("File actions", "Azioni sui file"),
    ("Copy the path", "Copia il percorso"),
    ("Copy to…", "Copia in…"),
    ("Move to…", "Sposta in…"),
    ("Open a terminal here", "Apri un terminale qui"),
    ("Put in a ZIP", "Metti in uno ZIP"),
    ("Choose a folder", "Scegli una cartella"),
    ("Terminal opened", "Terminale aperto"),
    ("Path copied", "Percorso copiato"),
    ("In the folder you chose", "Nella cartella scelta"),
    ("Rotate left", "Ruota a sinistra"),
    ("Rotate right", "Ruota a destra"),
    ("Black and white", "Bianco e nero"),
    ("Crop to a shape…", "Ritaglia in una forma…"),
    ("Trim…", "Taglia…"),
    ("Split into pages", "Dividi in pagine"),
    ("Keep only some pages…", "Tieni solo alcune pagine…"),
    ("Protect with a password…", "Proteggi con password…"),
    ("Remove the password…", "Togli la password…"),
    ("Extract here", "Estrai qui"),
    ("Join into one video", "Unisci in un solo video"),
    ("From", "Da"),
    ("To", "A"),
    ("For example 0:30 and 1:45. Only that part is kept.", "Ad esempio 0:30 e 1:45. Resta solo quella parte."),
    ("For example 1-3, 7. Only these pages are kept.", "Ad esempio 1-3, 7. Restano solo queste pagine."),
    ("Shape", "Forma"),
    ("The middle of the picture is kept", "Si tiene il centro dell'immagine"),
    ("Square · 1:1", "Quadrato · 1:1"),
    ("Portrait · 4:5", "Verticale · 4:5"),
    ("Photo · 3:2", "Foto · 3:2"),
    ("Classic · 4:3", "Classico · 4:3"),
    ("Wide · 16:9", "Panoramico · 16:9"),
    ("Story · 9:16", "Storia · 9:16"),
    ("Password", "Password"),
    ("Needed to open the PDF. Keep it somewhere safe: it can't be recovered.", "Servirà per aprire il PDF. Conservala bene: non si può recuperare."),
    ("The password that opens this PDF now.", "La password che apre questo PDF adesso."),
    ("wrong password", "password errata"),
    ("the archive is empty", "l'archivio è vuoto"),
    ("Text (TXT)", "Testo (TXT)"),
    ("Web page (HTML)", "Pagina web (HTML)"),
];

/// Translates a fixed string.
pub fn t(en: &str) -> String {
    if italian() {
        if let Some((_, it)) = IT.iter().find(|(k, _)| *k == en) {
            return (*it).to_string();
        }
    }
    en.to_string()
}

/// Translates a template and fills `{name}` placeholders.
pub fn tf(en: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = t(en);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), &v.to_string());
    }
    s
}
