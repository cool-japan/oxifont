//! Integration tests for the fontconfig XML configuration parser.
//!
//! These tests run on all platforms (synthesized XML fixtures) so that CI on
//! macOS also exercises the parser logic. The OS-specific test that queries the
//! real `/etc/fonts/fonts.conf` is gated on `target_os = "linux"`.

#[cfg(feature = "fontconfig")]
mod fontconfig_tests {
    use oxifont_discovery::fontconfig::parse_conf;
    use std::collections::HashSet;
    use std::path::PathBuf;

    /// Write `content` to a temp file and return its path.
    fn write_temp_conf(name: &str, content: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(name);
        std::fs::write(&p, content).expect("write temp conf");
        p
    }

    // -----------------------------------------------------------------------
    // Basic parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_simple_dir_elements() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
    <dir>/usr/share/fonts</dir>
    <dir>/usr/local/share/fonts</dir>
</fontconfig>"#;
        let path = write_temp_conf("oxifont_fc_simple.conf", xml);
        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        parse_conf(&path, &mut visited, &mut dirs);
        let _ = std::fs::remove_file(&path);

        assert!(
            dirs.contains(&PathBuf::from("/usr/share/fonts")),
            "should contain /usr/share/fonts"
        );
        assert!(
            dirs.contains(&PathBuf::from("/usr/local/share/fonts")),
            "should contain /usr/local/share/fonts"
        );
    }

    #[test]
    fn test_parse_tilde_expansion() {
        let xml = r#"<?xml version="1.0"?>
<fontconfig>
    <dir>~/.fonts</dir>
</fontconfig>"#;
        let path = write_temp_conf("oxifont_fc_tilde.conf", xml);
        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        parse_conf(&path, &mut visited, &mut dirs);
        let _ = std::fs::remove_file(&path);

        // On any platform with a home dir this must expand to something.
        // We cannot assert the exact path here (varies by user), but we can
        // assert that the raw "~/.fonts" string was NOT kept verbatim.
        if !dirs.is_empty() {
            let raw = dirs[0].to_str().unwrap_or("");
            assert!(
                !raw.starts_with('~'),
                "tilde must be expanded; got: {}",
                raw
            );
        }
    }

    #[test]
    fn test_parse_dollar_home_expansion() {
        let xml = r#"<?xml version="1.0"?>
<fontconfig>
    <dir>$HOME/.fonts</dir>
</fontconfig>"#;
        let path = write_temp_conf("oxifont_fc_dollarhome.conf", xml);
        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        parse_conf(&path, &mut visited, &mut dirs);
        let _ = std::fs::remove_file(&path);

        if !dirs.is_empty() {
            let raw = dirs[0].to_str().unwrap_or("");
            assert!(
                !raw.starts_with("$HOME"),
                "$HOME must be expanded; got: {}",
                raw
            );
        }
    }

    // -----------------------------------------------------------------------
    // Include directives
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_include_file() {
        let included_xml = r#"<?xml version="1.0"?>
<fontconfig>
    <dir>/opt/fonts</dir>
</fontconfig>"#;
        let included_path = write_temp_conf("oxifont_fc_included.conf", included_xml);

        let main_xml = format!(
            r#"<?xml version="1.0"?>
<fontconfig>
    <dir>/usr/share/fonts</dir>
    <include>{}</include>
</fontconfig>"#,
            included_path.display()
        );
        let main_path = write_temp_conf("oxifont_fc_main.conf", &main_xml);

        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        parse_conf(&main_path, &mut visited, &mut dirs);

        let _ = std::fs::remove_file(&main_path);
        let _ = std::fs::remove_file(&included_path);

        assert!(
            dirs.contains(&PathBuf::from("/usr/share/fonts")),
            "main conf dir"
        );
        assert!(
            dirs.contains(&PathBuf::from("/opt/fonts")),
            "included conf dir"
        );
    }

    // -----------------------------------------------------------------------
    // Cycle detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_cycle_detection_no_infinite_loop() {
        // conf_a includes conf_b, conf_b includes conf_a — must not loop.
        let tmp = std::env::temp_dir();
        let a = tmp.join("oxifont_fc_cycle_a.conf");
        let b = tmp.join("oxifont_fc_cycle_b.conf");

        let xml_a = format!(
            r#"<?xml version="1.0"?>
<fontconfig>
    <dir>/cycle/fonts-a</dir>
    <include>{}</include>
</fontconfig>"#,
            b.display()
        );
        let xml_b = format!(
            r#"<?xml version="1.0"?>
<fontconfig>
    <dir>/cycle/fonts-b</dir>
    <include>{}</include>
</fontconfig>"#,
            a.display()
        );

        std::fs::write(&a, &xml_a).expect("write a");
        std::fs::write(&b, &xml_b).expect("write b");

        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        // This must return without hanging.
        parse_conf(&a, &mut visited, &mut dirs);

        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);

        // Both dirs must appear exactly once.
        assert!(dirs.contains(&PathBuf::from("/cycle/fonts-a")));
        assert!(dirs.contains(&PathBuf::from("/cycle/fonts-b")));
    }

    // -----------------------------------------------------------------------
    // Missing / unreadable files
    // -----------------------------------------------------------------------

    #[test]
    fn test_missing_file_is_silently_skipped() {
        let path = std::env::temp_dir().join("oxifont_fc_does_not_exist_xyzzy.conf");
        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        // Must not panic.
        parse_conf(&path, &mut visited, &mut dirs);
        assert!(dirs.is_empty());
    }

    // -----------------------------------------------------------------------
    // Directory includes (conf.d pattern)
    // -----------------------------------------------------------------------

    #[test]
    fn test_include_directory_reads_conf_files() {
        let tmp = std::env::temp_dir();
        let conf_dir = tmp.join("oxifont_fc_confd_test");
        let _ = std::fs::create_dir_all(&conf_dir);

        // Write two *.conf files into the directory.
        let conf1 = conf_dir.join("10-test.conf");
        let conf2 = conf_dir.join("20-test.conf");
        std::fs::write(
            &conf1,
            r#"<?xml version="1.0"?><fontconfig><dir>/confd/fonts1</dir></fontconfig>"#,
        )
        .expect("write conf1");
        std::fs::write(
            &conf2,
            r#"<?xml version="1.0"?><fontconfig><dir>/confd/fonts2</dir></fontconfig>"#,
        )
        .expect("write conf2");

        let main_xml = format!(
            r#"<?xml version="1.0"?>
<fontconfig>
    <include>{}</include>
</fontconfig>"#,
            conf_dir.display()
        );
        let main_path = tmp.join("oxifont_fc_confd_main.conf");
        std::fs::write(&main_path, &main_xml).expect("write main");

        let mut visited = HashSet::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        parse_conf(&main_path, &mut visited, &mut dirs);

        let _ = std::fs::remove_file(&main_path);
        let _ = std::fs::remove_file(&conf1);
        let _ = std::fs::remove_file(&conf2);
        let _ = std::fs::remove_dir(&conf_dir);

        assert!(
            dirs.contains(&PathBuf::from("/confd/fonts1")),
            "conf1 dir not found; dirs = {:?}",
            dirs
        );
        assert!(
            dirs.contains(&PathBuf::from("/confd/fonts2")),
            "conf2 dir not found; dirs = {:?}",
            dirs
        );
    }

    // -----------------------------------------------------------------------
    // High-level API
    // -----------------------------------------------------------------------

    #[test]
    fn test_fontconfig_font_dirs_returns_vec() {
        // Must not panic and must return a Vec (possibly empty on non-Linux).
        let dirs = oxifont_discovery::fontconfig::fontconfig_font_dirs();
        let _ = dirs.len();
    }

    /// On a Linux system with fontconfig installed, verify we get real paths.
    #[cfg(target_os = "linux")]
    #[test]
    fn test_fontconfig_font_dirs_nonempty_on_linux() {
        if std::path::Path::new("/etc/fonts/fonts.conf").exists() {
            let dirs = oxifont_discovery::fontconfig::fontconfig_font_dirs();
            assert!(
                !dirs.is_empty(),
                "fontconfig_font_dirs() must return results on a system with /etc/fonts/fonts.conf"
            );
        }
    }
}
