// The share link's wire contract, from the Kotlin side.
//
// Since 2026-08-01 the phone does not build a share link itself: `ui/Church.kt`
// asks the core (`plumbline_share_url_json`) and reads the answer. That makes
// `ShareRequest` and `Share` the whole Android surface of the feature, and a
// field name that drifts from the wire is the way it breaks — kotlinx ignores
// keys it does not know, so a renamed field goes quietly to its default and the
// reader gets a link with no church in it and a Church button labelled "Your
// church".
//
// The golden strings below are the real `plumbline_share_url_json` answers,
// captured from the built cdylib. A JVM unit test cannot load the native library
// (see CLAUDE.md), so pinning its output is how this shell checks it still
// speaks the same JSON.

package dev.plumbline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlinx.serialization.encodeToString

class ShareWireTest {

    /** The core's answer for a reader with no church set. */
    private val plain =
        """{"url":"https://plumblinebible.org/","base":"https://plumblinebible.org/",""" +
            """"church":{"name":"","info":"","url":""},"hasChurch":false,""" +
            """"title":"Your church","siteUrl":null}"""

    /** …and for one who has. */
    private val withChurch =
        """{"url":"https://plumblinebible.org/?church=Grace+Bible+Church&churchInfo=Sundays+10am""" +
            """&churchUrl=https%3A%2F%2Fexample.org","base":"https://plumblinebible.org/",""" +
            """"church":{"name":"Grace Bible Church","info":"Sundays 10am","url":"https://example.org"},""" +
            """"hasChurch":true,"title":"Grace Bible Church: Sundays 10am","siteUrl":"https://example.org"}"""

    @Test
    fun the_plain_app_link_reads_as_no_church() {
        val s = parseWire<Share>(plain)
        assertEquals("https://plumblinebible.org/", s.url)
        assertEquals("https://plumblinebible.org/", s.base)
        assertFalse("nothing was set, so there is no church to show", s.hasChurch)
        assertEquals("Your church", s.title)
        assertNull(s.siteUrl)
    }

    @Test
    fun every_field_of_a_share_reaches_the_shell() {
        val s = parseWire<Share>(withChurch)
        assertTrue("a church was set, so the Church button and the 'with …' line show", s.hasChurch)
        assertEquals("Grace Bible Church", s.church.name)
        assertEquals("Sundays 10am", s.church.info)
        assertEquals("https://example.org", s.church.url)
        assertEquals("Grace Bible Church: Sundays 10am", s.title)
        assertEquals("https://example.org", s.siteUrl)
        assertTrue(
            "all three church fields must ride the link the QR encodes: ${s.url}",
            s.url.contains("church=Grace+Bible+Church") &&
                s.url.contains("churchInfo=Sundays+10am") &&
                s.url.contains("churchUrl=https%3A%2F%2Fexample.org"),
        )
    }

    /** A site we refuse to open comes back as a null `siteUrl` while the church
     *  itself still travels — the Church button falls back to the label. */
    @Test
    fun a_site_we_will_not_open_arrives_as_no_site() {
        val s = parseWire<Share>(
            """{"url":"https://plumblinebible.org/?church=Grace&churchUrl=javascript%3Aalert%281%29",""" +
                """"base":"https://plumblinebible.org/","church":{"name":"Grace","info":"","url":"javascript:alert(1)"},""" +
                """"hasChurch":true,"title":"Grace","siteUrl":null}""",
        )
        assertNull("javascript: must never reach an Intent", s.siteUrl)
        assertEquals("Grace", s.title)
    }

    /** The request the shell sends has to use the keys the core reads. Defaults
     *  are omitted on purpose (kotlinx `encodeDefaults = false`); the core's
     *  fields are all `#[serde(default)]`, so absence is the same as the
     *  default — but a MISSPELLED key would be silently ignored, which is
     *  exactly the failure this pins. */
    @Test
    fun the_request_uses_the_keys_the_core_reads() {
        val json = PlumblineJson.encodeToString(
            ShareRequest(
                church = ChurchState(name = "Grace", info = "Sun", url = "https://x.org"),
                startAsNewBeliever = true,
                at = "Ps 23:1",
            ),
        )
        assertEquals(
            """{"church":{"name":"Grace","info":"Sun","url":"https://x.org"},""" +
                """"startAsNewBeliever":true,"at":"Ps 23:1"}""",
            json,
        )
    }

    /** `linkAtVerse` is the only caller that supplies a base — Present holds a
     *  finished link and wants the opening verse added to it. */
    @Test
    fun a_base_is_sent_only_when_there_is_one() {
        assertEquals("{}", PlumblineJson.encodeToString(ShareRequest()))
        assertEquals(
            """{"base":"https://plumblinebible.org/?church=Grace","at":"Ps 23:1"}""",
            PlumblineJson.encodeToString(
                ShareRequest(base = "https://plumblinebible.org/?church=Grace", at = "Ps 23:1"),
            ),
        )
    }
}
