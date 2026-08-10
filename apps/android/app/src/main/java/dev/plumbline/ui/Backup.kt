package dev.plumbline.ui

// Study-data backup/restore: the authored home dirs as a zip via
// SAF. The archive layout is shared with the web shell's Settings backup —
// authored dirs at the zip root, the config under ".config/" — so one zip
// restores across devices.

import android.app.Activity
import android.content.Context
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.plumbline.isTempName
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.time.Instant
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

/** Authored dirs, home-relative — what a backup carries. Must stay in step with
 *  the web shell's USER_DIRS (apps/web/src/engine/home.ts): the archive layout is
 *  shared, so a dir missing here is a dir that silently doesn't cross devices. */
private val BACKUP_DIRS = listOf("tags", "threads", "weaves", "notes", "memory", "reading", "plans")

/** Where a restore unpacks before anything touches the live tree. Inside the
 *  home on purpose: the move-in is a rename, and a rename is only atomic within
 *  one filesystem (the cache dir can be a different one). */
private const val RESTORE_STAGE = ".restore-tmp"

/** One restore at a time, because [RESTORE_STAGE] is ONE directory for the
 *  whole process and the first thing a restore does is wipe it. The I/O runs
 *  on a background dispatcher, so two taps can overlap: two overlapping
 *  restores would have the second's `deleteRecursively` delete the first's
 *  staged files out from under it, and the first would then fail its
 *  did-everything-unpack check, having already told the reader nothing.
 *  Serialized here rather than in the UI so the invariant belongs to the thing
 *  that owns the directory (ConcurrentRestoreTest). */
private val restoreLock = Any()

/** Archives written before the Plumbline rename carry the config under
 *  "pure-study/"; the live home uses "plumbline/". Restore-side only — nothing
 *  writes the old name back, so this is a read shim for old zips, not a
 *  second identity. Without it an older backup silently drops the user's
 *  settings (the authored dirs above are unaffected: their names never moved). */
private fun String.currentConfigDir(): String =
    if (startsWith("pure-study/")) "plumbline/" + removePrefix("pure-study/") else this

/** Where a backup-zip entry lands, home-relative — or null to skip it. A zip is
 *  untrusted input and can name anything at all, so this is the single place
 *  that decides, and it is pure so every decision is testable (RestorePlanTest).
 *  Absolute paths, `..` traversal and anything outside the authored dirs are
 *  refused outright; the legacy config prefix is remapped on the way through. */
internal fun restoreDestination(entryName: String): String? {
    // Zip names are '/'-separated by spec, but a Windows-authored archive can
    // carry backslashes. Treat those as separators too, so such a name is vetted
    // segment by segment instead of slipping through as one long filename.
    val name = entryName.replace('\\', '/')
    if (name.startsWith("/")) return null // absolute — never
    val parts = name.split('/')
    // An empty segment covers a trailing slash (a directory entry), a doubled
    // slash, and bare prefixes like ".config/".
    if (parts.any { it.isEmpty() || it == "." || it == ".." }) return null
    if (parts.size < 2) return null // a root-level file (the manifest) restores nothing
    // A stranded temp in an OLDER zip — written before the backup walk learned to
    // skip them — must not be planted on this device either. Refusing on the way
    // in is what makes the fix retroactive.
    if (isTempName(parts.last())) return null
    return when (parts[0]) {
        in BACKUP_DIRS -> name
        // ".config/plumbline/…" → the XDG config dir (= home here).
        ".config" -> name.removePrefix(".config/").currentConfigDir()
        else -> null
    }
}

/** The restore itself, with no Android in it: [openZip] hands over the archive
 *  bytes, [home] is the live tree. Returns the file count (0 = nothing in the
 *  archive belongs to us); throws if the archive is unusable.
 *
 *  All-or-nothing. The whole archive is unpacked into [RESTORE_STAGE] and
 *  verified there, and only then does each file move into the live tree — the
 *  staged file *is* the temp file of store.rs's temp+rename, so publishing it is
 *  one atomic rename and no destination is ever half-written. A truncated
 *  download or a bad CRC leaves the reader's study data exactly as it was;
 *  streaming entries straight over the home would leave it half-overwritten
 *  with no way back. */
internal fun restoreZipInto(home: File, openZip: () -> InputStream): Int = synchronized(restoreLock) {
    val stage = File(home, RESTORE_STAGE)
    stage.deleteRecursively() // an attempt that died mid-flight leaves one behind
    try {
        val stageCanon = stage.canonicalPath + File.separator
        // Unpack. Insertion-ordered and de-duplicated, so a zip naming the same
        // file twice restores it once, the later entry winning as before.
        val staged = LinkedHashSet<String>()
        openZip().use { ins ->
            ZipInputStream(ins).use { zip ->
                var next = zip.nextEntry
                while (next != null) {
                    val e: ZipEntry = next
                    val rel = restoreDestination(e.name)
                    if (rel != null) {
                        val out = File(stage, rel)
                        // Belt to the sanitiser's braces — whatever the name did,
                        // the file it produced must sit under the staging dir.
                        require(out.canonicalPath.startsWith(stageCanon)) {
                            "backup entry escapes the staging dir: ${e.name}"
                        }
                        out.parentFile?.mkdirs()
                        FileOutputStream(out).use { o ->
                            zip.copyTo(o)
                            o.fd.sync() // durable before the rename that publishes it
                        }
                        // closeEntry checks the entry's CRC-32 and length, so a
                        // corrupt or truncated archive throws right here — before
                        // the live tree has been touched at all.
                        zip.closeEntry()
                        if (e.size >= 0 && out.length() != e.size) {
                            throw IOException("backup entry ${e.name} is ${out.length()} bytes, not ${e.size}")
                        }
                        staged += rel
                    } else {
                        zip.closeEntry()
                    }
                    next = zip.nextEntry
                }
            }
        }
        if (staged.isEmpty()) return 0

        // Verify the staged set is whole, and that every destination really is
        // inside the home, before a single live file changes.
        val homeCanon = home.canonicalPath + File.separator
        for (rel in staged) {
            if (!File(stage, rel).isFile) throw IOException("backup entry $rel did not unpack")
            require(File(home, rel).canonicalPath.startsWith(homeCanon)) {
                "backup entry escapes the home: $rel"
            }
        }

        // Move in: one rename per file, straight out of the staging dir. The
        // stage is inside the home, so this is a same-filesystem rename — atomic,
        // and it replaces the destination outright. Renaming rather than copying
        // also means no temp file is ever left in an authored dir for the next
        // backup to sweep up.
        var count = 0
        for (rel in staged) {
            val target = File(home, rel)
            target.parentFile?.mkdirs()
            if (!File(stage, rel).renameTo(target)) throw IOException("could not put $rel in place")
            count++
        }
        return count
    } finally {
        stage.deleteRecursively()
    }
}

/** Zip the authored dirs + config to [uri]. Returns the file count. */
fun writeBackupZip(context: Context, home: File, uri: Uri): Int {
    var count = 0
    context.contentResolver.openOutputStream(uri)!!.use { out ->
        ZipOutputStream(out).use { zip ->
            fun add(file: File, entryName: String) {
                zip.putNextEntry(ZipEntry(entryName))
                file.inputStream().use { it.copyTo(zip) }
                zip.closeEntry()
                count++
            }
            for (d in BACKUP_DIRS) {
                val dir = File(home, d)
                if (!dir.isDirectory) continue
                // Not stranded temps. Android is the shell that actually strands
                // them — a process kill between write and rename is ordinary
                // here — and a temp that rides into a zip is restored onto the
                // next device as a fixture nothing ever removes.
                dir.walkTopDown().filter { it.isFile && !isTempName(it.name) }.forEach { f ->
                    add(f, f.relativeTo(home).invariantSeparatorsPath)
                }
            }
            // Config lives at $XDG_CONFIG_HOME/plumbline (= home on Android);
            // it travels under ".config/" (the web home's layout).
            val cfg = File(home, "plumbline/config.json")
            if (cfg.isFile) add(cfg, ".config/plumbline/config.json")
            zip.putNextEntry(ZipEntry("plumbline-backup.json"))
            zip.write("""{"format":1,"app":"android","exported":"${Instant.now()}"}""".toByteArray())
            zip.closeEntry()
        }
    }
    return count
}

/** Restore the backup zip at [uri] into the home — the SAF wrapper around
 *  [restoreZipInto]. Returns the file count (0 = not a plumbline backup). */
fun restoreBackupZip(context: Context, home: File, uri: Uri): Int =
    restoreZipInto(home) { context.contentResolver.openInputStream(uri)!! }

/** The two Settings rows: back up to a zip, restore from one (then the
 *  activity recreates so the engine re-opens over the restored home).
 *
 *  BOTH ZIPS RUN OFF THE MAIN THREAD: a backup walks every authored
 *  dir and deflates it into a stream the content provider owns, and a restore
 *  unpacks, fsyncs and renames every entry — file I/O of unbounded size that
 *  must not sit on the thread that draws.
 *
 *  The `runCatching` stays INSIDE the dispatch, and that placement is the whole
 *  of the error contract. It turns a throw into the same -1 the reader was
 *  always told about; let it escape `withContext` instead and the failure would
 *  cancel the scope with no toast at all — a backup that silently did nothing,
 *  which is the one outcome worse than a slow one. The three restore verdicts
 *  (recreate / nothing-of-ours / failed) are unchanged, and both toasts are
 *  raised after the dispatch returns, back on the main thread. */
@Composable
fun BackupRestoreRows(palette: ReaderPalette) {
    val context = LocalContext.current
    val home = context.filesDir
    val scope = rememberCoroutineScope()

    val backupLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/zip"),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val n = withContext(Dispatchers.IO) {
                runCatching { writeBackupZip(context, home, uri) }.getOrElse { -1 }
            }
            Toast.makeText(
                context,
                if (n >= 0) Strings.plural("settings.backedUpN.one", "settings.backedUpN.other", n) else t("settings.backupFailedShort"),
                Toast.LENGTH_SHORT,
            ).show()
        }
    }
    val restoreLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val n = withContext(Dispatchers.IO) {
                runCatching { restoreBackupZip(context, home, uri) }.getOrElse { -1 }
            }
            when {
                n > 0 -> (context as? Activity)?.recreate()
                n == 0 -> Toast.makeText(context, t("settings.restoreNothing"), Toast.LENGTH_SHORT).show()
                else -> Toast.makeText(context, t("settings.restoreFailedShort"), Toast.LENGTH_SHORT).show()
            }
        }
    }

    Column {
        Text(
            t("settings.data"),
            color = palette.faded,
            fontSize = 12.sp,
        )
        Column(
            Modifier.fillMaxWidth().clickable {
                backupLauncher.launch("plumbline-backup-${Instant.now().toString().take(10)}.zip")
            }.padding(vertical = 6.dp),
        ) {
            Text(t("settings.backup"), color = palette.ink, fontSize = 15.sp)
        }
        Column(
            Modifier.fillMaxWidth().clickable {
                restoreLauncher.launch(arrayOf("application/zip", "application/octet-stream"))
            }.padding(vertical = 6.dp),
        ) {
            Text(t("settings.restore"), color = palette.ink, fontSize = 15.sp)
            Text(
                t("settings.dataDesc"),
                color = palette.faded,
                fontSize = 12.sp,
            )
        }
    }
}
