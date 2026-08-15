use eframe::egui::Context;
use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol};
use std::{path::PathBuf, sync::mpsc::Sender};

const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS_EVENT: u32 = u32::from_be_bytes(*b"odoc");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

struct OpenDocumentHandlerIvars {
    project_sender: Sender<PathBuf>,
    repaint: Context,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = OpenDocumentHandlerIvars]
    struct OpenDocumentHandler;

    unsafe impl NSObjectProtocol for OpenDocumentHandler {}

    impl OpenDocumentHandler {
        #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
        fn handle_open_documents(
            &self,
            event: &NSAppleEventDescriptor,
            _reply_event: &NSAppleEventDescriptor,
        ) {
            let documents: Option<Retained<NSAppleEventDescriptor>> = unsafe {
                msg_send![event, descriptorForKeyword: DIRECT_OBJECT]
            };
            let Some(documents) = documents else {
                return;
            };

            for index in 1..=documents.numberOfItems() {
                let Some(document) = documents.descriptorAtIndex(index) else {
                    continue;
                };
                let Some(url) = document.fileURLValue() else {
                    continue;
                };
                let Some(path) = url.path() else {
                    continue;
                };
                let _ = self
                    .ivars()
                    .project_sender
                    .send(PathBuf::from(path.to_string()));
            }
            self.ivars().repaint.request_repaint();
        }
    }
);

impl OpenDocumentHandler {
    fn new(
        project_sender: Sender<PathBuf>,
        repaint: Context,
        main_thread_marker: MainThreadMarker,
    ) -> Retained<Self> {
        let this = main_thread_marker
            .alloc()
            .set_ivars(OpenDocumentHandlerIvars {
                project_sender,
                repaint,
            });
        unsafe { msg_send![super(this), init] }
    }
}

pub fn install_open_document_handler(project_sender: Sender<PathBuf>, repaint: Context) {
    let main_thread_marker = MainThreadMarker::new().expect("open event handler must run on macOS");
    let handler = OpenDocumentHandler::new(project_sender, repaint, main_thread_marker);
    let manager = NSAppleEventManager::sharedAppleEventManager();
    let selector = Sel::register(c"handleOpenDocuments:withReplyEvent:");

    unsafe {
        let _: () = msg_send![
            &*manager,
            setEventHandler: &*handler,
            andSelector: selector,
            forEventClass: CORE_EVENT_CLASS,
            andEventID: OPEN_DOCUMENTS_EVENT
        ];
    }

    let _registration = Box::leak(Box::new((manager, handler)));
}
