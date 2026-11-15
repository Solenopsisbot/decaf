//! `java.lang.StackWalker` over the interpreter's own jar frames.
//!
//! Logging shims ask the stack who called them: WorldEdit's
//! `LogManagerCompat.getLogger()` walks `s -> s.skip(2).findFirst()` and names
//! its logger after the class it finds, throwing `AssertionError("No caller
//! found")` when the stream is empty. The interpreter's frames are the only
//! Java frames a native run has, so the walk answers exactly those, innermost
//! first, the way the JVM's walker lists them. Host-side frames (the loader
//! that invoked the entrypoint) do not exist natively and are not invented.
//!
//! The frame list is a thread-local stack pushed and popped by
//! `Runtime::invoke`: one native execution runs on one thread, and a nested
//! runtime (a callback fired from a hook) only adds real callers on top.

use super::*;
use std::cell::RefCell;

thread_local! {
    /// The jar frames currently executing on this thread, outermost first:
    /// `(internal class name, method name)`.
    static FRAMES: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

/// Record entry into one jar frame. Paired with [`leave`]; the stack never
/// grows past the interpreter's own recursion bound.
pub(crate) fn enter(class: &str, method: &str) {
    FRAMES.with(|frames| {
        frames
            .borrow_mut()
            .push((class.to_owned(), method.to_owned()));
    });
}

/// Record the return (value or error) of the innermost jar frame.
pub(crate) fn leave() {
    FRAMES.with(|frames| {
        frames.borrow_mut().pop();
    });
}

/// The frames a walk sees, innermost first.
pub(crate) fn snapshot() -> Vec<(String, String)> {
    FRAMES.with(|frames| frames.borrow().iter().rev().cloned().collect())
}

impl<H: Host> Runtime<'_, H> {
    /// Answer the `StackWalker` surface, or `None` for any other call.
    pub(super) fn invoke_stack_walker(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match (r.owner.as_str(), r.name.as_str()) {
            ("java/lang/StackWalker", "getInstance") => {
                let retain = match (r.descriptor.as_str(), args.first()) {
                    ("()Ljava/lang/StackWalker;", _) => false,
                    ("(I)Ljava/lang/StackWalker;", Some(Value::Int(depth))) if *depth >= 0 => false,
                    ("(Ljava/lang/StackWalker$Option;)Ljava/lang/StackWalker;", Some(option))
                        if crate::host_jdk_constant::stack_walker_option_name(option)
                            == Some("RETAIN_CLASS_REFERENCE") =>
                    {
                        true
                    }
                    ("(Ljava/util/Set;)Ljava/lang/StackWalker;", Some(set)) => {
                        if self.host_collection_kind(set).is_none() {
                            return Ok(None);
                        }
                        let mut retain = false;
                        for option in self.collection_elements(set)? {
                            match crate::host_jdk_constant::stack_walker_option_name(&option) {
                                Some("RETAIN_CLASS_REFERENCE") => retain = true,
                                Some(_) | None => return Ok(None),
                            }
                        }
                        retain
                    }
                    _ => return Ok(None),
                };
                Ok(Some(crate::host_jdk::stack_walker(retain)))
            }
            ("java/lang/StackWalker", "walk")
                if r.descriptor == "(Ljava/util/function/Function;)Ljava/lang/Object;" =>
            {
                let function = StdHost::argument(r, args, 0)?.clone();
                let retain = receiver
                    .and_then(crate::host_jdk::stack_walker_retain)
                    .ok_or_else(|| NativeError::Metadata("invalid StackWalker receiver".into()))?;
                // `walk`'s own frame is the JDK's, not the caller's; the first
                // frame the function sees is the method that called `walk`.
                // A top-level native entrypoint has no interpreter caller frame
                // to stand in for Java's launcher/main frame. Count that boundary
                // instead of returning a shortened stream.
                let snapshot = snapshot();
                if snapshot.len() < 2 {
                    return Ok(Some(self.host.unknown_operand_call(r)?));
                }
                let frames = snapshot
                    .iter()
                    .map(|(class, method)| {
                        crate::host_jdk::frame_value_with_retain(
                            HostKind::StackFrame,
                            class,
                            method,
                            retain,
                        )
                    })
                    .collect();
                let stream = self.new_stream("java/lang/StackWalker", "walk", frames);
                Ok(Some(
                    self.invoke_callback(&function, &[stream])?
                        .unwrap_or(Value::Ref(None)),
                ))
            }
            ("java/lang/StackWalker", "getCallerClass")
                if r.descriptor == "()Ljava/lang/Class;" =>
            {
                if receiver.and_then(crate::host_jdk::stack_walker_retain) != Some(true) {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/UnsupportedOperationException".into(),
                        message: None,
                        frame: None,
                    });
                }
                // The interpreter snapshot already excludes the JDK helper
                // frame, so frame 0 is the jar method that called us.
                match snapshot().first() {
                    Some((class, _)) => Ok(Some(crate::host_class::class_value(class))),
                    None => Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalCallerException".into(),
                        message: Some("no caller frame".into()),
                        frame: None,
                    }),
                }
            }
            ("java/lang/StackWalker$StackFrame", name) => {
                let Some((class, method)) = receiver
                    .and_then(|frame| crate::host_jdk::frame_parts(frame, HostKind::StackFrame))
                else {
                    return Ok(None);
                };
                if name == "getDeclaringClass"
                    && receiver.and_then(|frame| {
                        crate::host_jdk::frame_retain_class_reference(frame, HostKind::StackFrame)
                    }) != Some(true)
                {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/UnsupportedOperationException".into(),
                        message: None,
                        frame: None,
                    });
                }
                Ok(match name {
                    "getClassName" => Some(Value::Ref(Some(class.replace('/', ".")))),
                    "getMethodName" => Some(Value::Ref(Some(method))),
                    "getDeclaringClass" => Some(crate::host_class::class_value(&class)),
                    _ => None,
                })
            }
            _ => Ok(None),
        }
    }
}
