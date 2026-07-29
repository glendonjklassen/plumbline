package dev.plumbline.ui

// Study-data backup/restore (2026-07-25): the authored home dirs as a zip via
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.io.File
import java.time.Instant
import java.util.zip.ZipEntry
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

/** Authored dirs, home-relative — what a backup carries. Must stay in step with
 *  the web shell's USER_DIRS (apps/web/src/engine/home.ts): the archive layout is
 *  shared, so a dir missing here is a dir that silently doesn't cross devices. */
private val BACKUP_DIRS = listOf("tags", "threads", "weaves", "notes", "memory", "reading")

/** Archives written before the Plumbline rename carry the config under
 *  "pure-study/"; the live home uses "plumbline/". Restore-side only — nothing
 *  writes the old name back, so this is a read shim for old zips, not a
 *  second identity. Without it an older backup silently drops the user's
 *  settings (the authored dirs above are unaffected: their names never moved). */
private fun String.currentConfigDir(): String =
    if (startsWith("pure-study/")) "plumbline/" + removePrefix("pure-study/") else this

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
                dir.walkTopDown().filter { it.isFile }.forEach { f ->
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

/** Restore a backup zip into the home. Returns the file count (0 = not a
 *  plumbline backup). Entries are path-filtered to the authored dirs. */
fun restoreBackupZip(context: Context, home: File, uri: Uri): Int {
    var count = 0
    val homeCanon = home.canonicalPath + File.separator
    context.contentResolver.openInputStream(uri)!!.use { ins ->
        ZipInputStream(ins).use { zip ->
            var e = zip.nextEntry
            while (e != null) {
                val name = e.name
                val target: File? = when {
                    e.isDirectory || name.contains("..") -> null
                    BACKUP_DIRS.any { name.startsWith("$it/") } -> File(home, name)
                    // ".config/plumbline/…" → the XDG config dir (= home here).
                    name.startsWith(".config/") ->
                        File(home, name.removePrefix(".config/").currentConfigDir())
                    else -> null
                }
                if (target != null && target.canonicalPath.startsWith(homeCanon)) {
                    target.parentFile?.mkdirs()
                    target.outputStream().use { zip.copyTo(it) }
                    count++
                }
                zip.closeEntry()
                e = zip.nextEntry
            }
        }
    }
    return count
}

/** The two Settings rows: back up to a zip, restore from one (then the
 *  activity recreates so the engine re-opens over the restored home). */
@Composable
fun BackupRestoreRows(palette: ReaderPalette) {
    val context = LocalContext.current
    val home = context.filesDir

    val backupLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/zip"),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val n = runCatching { writeBackupZip(context, home, uri) }.getOrElse { -1 }
        Toast.makeText(
            context,
            if (n >= 0) "Backed up $n files" else "Backup failed",
            Toast.LENGTH_SHORT,
        ).show()
    }
    val restoreLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val n = runCatching { restoreBackupZip(context, home, uri) }.getOrElse { -1 }
        when {
            n > 0 -> (context as? Activity)?.recreate()
            n == 0 -> Toast.makeText(context, "No study data found in that zip", Toast.LENGTH_SHORT).show()
            else -> Toast.makeText(context, "Restore failed", Toast.LENGTH_SHORT).show()
        }
    }

    Column {
        Text(
            "Your study data — notes, tags, threads, weaves, memorization",
            color = palette.faded,
            fontSize = 12.sp,
        )
        Column(
            Modifier.fillMaxWidth().clickable {
                backupLauncher.launch("plumbline-backup-${Instant.now().toString().take(10)}.zip")
            }.padding(vertical = 6.dp),
        ) {
            Text("Back up (.zip)…", color = palette.ink, fontSize = 15.sp)
        }
        Column(
            Modifier.fillMaxWidth().clickable {
                restoreLauncher.launch(arrayOf("application/zip", "application/octet-stream"))
            }.padding(vertical = 6.dp),
        ) {
            Text("Restore from backup…", color = palette.ink, fontSize = 15.sp)
            Text(
                "The same zip restores on the web and Android. Same-name items are replaced.",
                color = palette.faded,
                fontSize = 12.sp,
            )
        }
    }
}
