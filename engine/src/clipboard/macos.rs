#[cfg(target_os = "macos")]
pub mod macos_files {
    use objc2::runtime::ProtocolObject;
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::panic;

    pub fn read_files_from_pasteboard() -> Option<Vec<String>> {
        let result = panic::catch_unwind(|| {
            let pboard = NSPasteboard::generalPasteboard();
            let classes = NSArray::from_slice(&[NSURL::class()]);
            let objects = unsafe { pboard.readObjectsForClasses_options(&classes, None) }?;

            let mut paths = Vec::new();
            for i in 0..objects.count() {
                let obj = objects.objectAtIndex(i);
                if let Ok(url) = obj.downcast::<NSURL>() {
                    if url.isFileURL() {
                        if let Some(path) = url.path() {
                            paths.push(path.to_string());
                        }
                    }
                }
            }

            if paths.is_empty() {
                None
            } else {
                Some(paths)
            }
        });

        match result {
            Ok(paths) => paths,
            Err(_) => None,
        }
    }

    pub fn write_files_to_pasteboard(paths: &[String]) -> Result<(), String> {
        let result = panic::catch_unwind(|| {
            if paths.is_empty() {
                return true;
            }

            let pboard = NSPasteboard::generalPasteboard();
            pboard.clearContents();

            let mut urls = Vec::new();
            for p in paths {
                let ns_path = NSString::from_str(p);
                let url = NSURL::fileURLWithPath(&ns_path);
                urls.push(url);
            }

            let proto_objects: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
                urls.iter().map(|u| ProtocolObject::from_ref(&**u)).collect();
            let array = NSArray::from_slice(&proto_objects);
            pboard.writeObjects(&array);
            true
        });

        match result {
            Ok(true) => Ok(()),
            Ok(false) => Err("Failed to write files to pasteboard".into()),
            Err(_) => Err("Exception while writing files to pasteboard".into()),
        }
    }
}
