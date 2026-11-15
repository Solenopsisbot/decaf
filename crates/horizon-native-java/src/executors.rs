//! `java.util.concurrent` executors, run on the runtime's own thread.
//!
//! A mod that does its init, update check or metric work on a pool builds it
//! with an `Executors` factory and hands the work to `execute`/`submit`. The
//! bounded runtime has one thread, so the task runs inline, before the call
//! returns; the `Future` a `submit` answers is already complete, and
//! `shutdown`/`isShutdown`/`awaitTermination` report the state the shutdown
//! calls set.
//!
//! Named departures, named in the PR that added this file:
//! * `execute`/`submit`/`schedule` run the task **immediately, on the calling
//!   thread**, not concurrently on a pool thread. A jar that reads a flag its
//!   task sets *after* `execute` returns sees it already set (in Java the task
//!   may not have started); the answers to `get`, `isDone` and `shutdown` are
//!   the completed ones.
//! * a scheduled task runs at once instead of after its delay.
//! * `awaitTermination` answers the current shutdown state instead of waiting
//!   (the runtime never blocks); `scheduleAtFixedRate`/`scheduleWithFixedDelay`
//!   are not modelled (a periodic task would run to the budget), so they stay
//!   counted unknowns.
//! * a `ThreadFactory` is modelled only as far as making an unstarted
//!   `Thread`; the pool never creates one, and `Thread.start` stays counted.

use super::*;

/// The synthetic field an executor marker keeps its shutdown flag under.
const SHUTDOWN_FIELD: &str = "#executorShutdown";
/// The synthetic field a future marker keeps its completed value under.
const FUTURE_VALUE: &str = "#executorValue";
const FUTURE_FAILURE: &str = "#executorFailure";

const EXECUTOR: &str = "java/util/concurrent/Executor";
/// The JDK executor classes a jar constructs with `new`.
const JDK_EXECUTORS: &[&str] = &[
    "java/util/concurrent/ThreadPoolExecutor",
    "java/util/concurrent/ScheduledThreadPoolExecutor",
    "java/util/concurrent/ForkJoinPool",
];
const EXECUTOR_SERVICE: &str = "java/util/concurrent/ExecutorService";
const SCHEDULED_EXECUTOR_SERVICE: &str = "java/util/concurrent/ScheduledExecutorService";
const SCHEDULED_FUTURE: &str = "java/util/concurrent/ScheduledFuture";
const FUTURE: &str = "java/util/concurrent/Future";
const COMPLETABLE_FUTURE: &str = "java/util/concurrent/CompletableFuture";
const COMPLETION_STAGE: &str = "java/util/concurrent/CompletionStage";
const RUNNABLE_RUN: &str = "()V";
const CALLABLE_CALL: &str = "()Ljava/lang/Object;";

fn run_ref() -> MethodRef {
    MethodRef {
        owner: "java/lang/Runnable".into(),
        name: "run".into(),
        descriptor: RUNNABLE_RUN.into(),
    }
}

/// Answer one executor, factory or future call, or `None`.
pub(super) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match r.owner.as_str() {
        "java/util/concurrent/Executors" => invoke_executors(rt, r, args),
        COMPLETABLE_FUTURE if receiver.is_none() => invoke_completed_future(rt, r, args),
        "java/util/concurrent/Callable" => invoke_callable(rt, r, receiver),
        EXECUTOR | EXECUTOR_SERVICE | SCHEDULED_EXECUTOR_SERVICE => {
            invoke_executor(rt, r, receiver, args)
        }
        FUTURE
        | SCHEDULED_FUTURE
        | COMPLETABLE_FUTURE
        | COMPLETION_STAGE
        | "java/util/concurrent/RunnableFuture" => invoke_future(rt, r, receiver, args),
        _ => Ok(None),
    }
}

fn invoke_completed_future<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.name == "completedFuture"
        && r.descriptor == "(Ljava/lang/Object;)Ljava/util/concurrent/CompletableFuture;"
    {
        return Ok(Some(complete_future(
            rt,
            args.first().cloned().unwrap_or(Value::Ref(None)),
        )));
    }
    if r.name == "failedFuture"
        && r.descriptor == "(Ljava/lang/Throwable;)Ljava/util/concurrent/CompletableFuture;"
    {
        let failure = args.first().cloned().unwrap_or(Value::Ref(None));
        if failure == Value::Ref(None) {
            return Err(native_npe());
        }
        return Ok(Some(failed_future(rt, failure)));
    }
    // Async factories require a real executor and stay counted.
    Ok(None)
}

fn invoke_executors<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    // A fresh pool per call (`host_executors.rs`); a scheduled one is a
    // `ScheduledExecutorService`.
    let scheduled = r.descriptor.ends_with("ScheduledExecutorService;");
    let mut pool = || {
        let marker = rt.new_object(if scheduled {
            "scheduled-executor"
        } else {
            "executor"
        });
        host_executors::allocated(HostKind::Executor, marker)
    };
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "newSingleThreadExecutor"
            | "newCachedThreadPool"
            | "newSingleThreadScheduledExecutor"
            | "newWorkStealingPool"
            // A virtual thread per task is, for this runtime, the same
            // single-threaded service; the departure is the one above.
            | "newVirtualThreadPerTaskExecutor",
            "()Ljava/util/concurrent/ExecutorService;"
            | "()Ljava/util/concurrent/ScheduledExecutorService;",
        ) => pool(),
        (
            "newSingleThreadExecutor"
            | "newCachedThreadPool"
            | "newSingleThreadScheduledExecutor"
            | "newFixedThreadPool"
            | "newScheduledThreadPool",
            _,
        ) if r
            .descriptor
            .starts_with("(Ljava/util/concurrent/ThreadFactory;)")
            || r.descriptor.contains("ThreadFactory") =>
        {
            pool()
        }
        ("newFixedThreadPool", _)
        | ("newWorkStealingPool", "(I)Ljava/util/concurrent/ExecutorService;") => {
            // `newFixedThreadPool`'s `ThreadPoolExecutor` refuses a
            // non-positive maximum; the JDK 27 message is the real one.
            let Some(Value::Int(size)) = args.first() else {
                return Err(NativeError::Metadata(
                    "Executors.newFixedThreadPool needs an int".into(),
                ));
            };
            if *size <= 0 {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalArgumentException".into(),
                    message: Some("maximumPoolSize must be positive".into()),
                    frame: None,
                });
            }
            pool()
        }
        ("newScheduledThreadPool", _) => {
            let Some(Value::Int(size)) = args.first() else {
                return Err(NativeError::Metadata(
                    "Executors.newScheduledThreadPool needs an int".into(),
                ));
            };
            if *size < 0 {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalArgumentException".into(),
                    message: None,
                    frame: None,
                });
            }
            pool()
        }
        // `Executors.callable(runnable)` and `callable(runnable, result)`.
        (
            "callable",
            "(Ljava/lang/Runnable;)Ljava/util/concurrent/Callable;"
            | "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Callable;",
        ) => {
            // `Executors.callable(null)`: `Objects.requireNonNull(task)`.
            let Some(task) = args.first().filter(|task| **task != Value::Ref(None)) else {
                return Err(native_npe());
            };
            let result = r
                .descriptor
                .contains("Ljava/lang/Object;)")
                .then(|| args.get(1).cloned().unwrap_or(Value::Ref(None)));
            host_executors::callable(task, result.as_ref())
        }
        ("defaultThreadFactory" | "privilegedThreadFactory", _) => {
            let marker = rt.new_object("thread-factory");
            host_executors::allocated(HostKind::ThreadFactory, marker)
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn invoke_callable<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
) -> Result<Option<Value>, NativeError> {
    if (r.name.as_str(), r.descriptor.as_str()) != ("call", CALLABLE_CALL) {
        return Ok(None);
    }
    if receiver.is_none_or(|receiver| *receiver == Value::Ref(None)) {
        return Err(native_npe());
    }
    // `Executors.callable`: run the task, answer the result (or `null`).
    let Some((task, result)) = receiver.and_then(host_executors::callable_parts) else {
        return Ok(None);
    };
    run_runnable(rt, &task)?;
    Ok(Some(result.unwrap_or(Value::Ref(None))))
}

/// The text an executor's or a future's state is kept under: a handle's
/// allocation marker, or (as before) the marker of another object the call
/// names; `Ok(None)` for another handle, `Err` for `null`.
fn state_marker<H: Host>(
    rt: &Runtime<'_, H>,
    receiver: Option<&Value>,
    kind: HostKind,
) -> Result<Option<String>, NativeError> {
    match receiver {
        None | Some(Value::Ref(None)) => Err(native_npe()),
        // Only an object `new` built keeps its state under its key: a jar's
        // own (`AbstractExecutorService` subclass) or a JDK executor a jar
        // constructs (`new ThreadPoolExecutor(..)`), a handle either way
        // (#156 P12b). A `String` spelled like one's key
        // (`"java/util/concurrent/ThreadPoolExecutor#0"`) is no executor, so
        // its call goes on to be counted, never reaching a real pool's state.
        Some(value) if crate::jar_object::is_object(value) => {
            Ok(crate::jar_object::object_class(value)
                .is_some_and(|class| rt.class(class).is_some() || JDK_EXECUTORS.contains(&class))
                .then(|| {
                    crate::jar_object::object_key(value)
                        .unwrap_or_default()
                        .to_owned()
                }))
        }
        Some(value) => Ok(host_executors::allocation(value, kind).map(str::to_owned)),
    }
}

fn invoke_executor<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = state_marker(rt, receiver, HostKind::Executor)? else {
        return Ok(None);
    };
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("execute", "(Ljava/lang/Runnable;)V") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            Value::Void
        }
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            complete_future(rt, Value::Ref(None))
        }
        ("submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            complete_future(rt, args.get(1).cloned().unwrap_or(Value::Ref(None)))
        }
        ("submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            let result = run_callable(rt, task)?;
            complete_future(rt, result)
        }
        ("schedule", descriptor)
            if descriptor.starts_with("(Ljava/lang/Runnable;J")
                || descriptor.starts_with("(Ljava/util/concurrent/Callable;J") =>
        {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            let result = if r.descriptor.starts_with("(Ljava/util/concurrent/Callable;") {
                run_callable(rt, task)?
            } else {
                run_runnable(rt, task)?;
                Value::Ref(None)
            };
            complete_scheduled_future(rt, result)
        }
        ("shutdown" | "close", "()V") => {
            rt.set_executor_shutdown(&marker);
            Value::Void
        }
        ("shutdownNow", _) => {
            rt.set_executor_shutdown(&marker);
            collection_value("list", &[])
        }
        ("isShutdown" | "isTerminated", "()Z") => {
            Value::Int(i32::from(rt.executor_shutdown(&marker)))
        }
        // The runtime never blocks: a shutdown pool is already terminated.
        ("awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z") => {
            Value::Int(i32::from(rt.executor_shutdown(&marker)))
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// `ThreadFactory.newThread(runnable)` hands out an unstarted `Thread` marker.
pub(super) fn invoke_thread_factory<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Option<Value> {
    if receiver
        .and_then(|factory| host_executors::allocation(factory, HostKind::ThreadFactory))
        .is_none()
        || (r.name.as_str(), r.descriptor.as_str())
            != ("newThread", "(Ljava/lang/Runnable;)Ljava/lang/Thread;")
    {
        return None;
    }
    let thread = rt.new_object("java/lang/Thread");
    if let Some(key) = crate::jar_object::object_key(&thread) {
        let target = args.first().cloned().unwrap_or(Value::Ref(None));
        rt.fields.insert(format!("{key}|{THREAD_TARGET}"), target);
    }
    Some(thread)
}

/// The synthetic field a constructed thread keeps its `Runnable` under
/// (`null` when it was given none).
const THREAD_TARGET: &str = "#threadTarget";

/// `Thread`'s own members the runtime answers on a thread the jar constructed
/// (#194): `<init>` keeps the `Runnable` target (the host records the rest,
/// `parked_threads.rs`), and `Thread.run()` is Java 25's
/// `if (target != null) target.run()`. A jar subclass's override of `run()` is
/// dispatched to before this; a thread whose constructor did not run here is
/// left to the host, which counts it.
pub(super) fn invoke_thread<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(key) = receiver
        .and_then(crate::jar_object::object_key)
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    let field = format!("{key}|{THREAD_TARGET}");
    // `Thread`'s own members; `run()` also when the call names a jar subclass
    // that inherits it (`carrier.run()` on `class Carrier extends Thread`):
    // only a thread constructed here keeps a target.
    if r.owner != "java/lang/Thread" && !(r.name == "run" && rt.fields.contains_key(&field)) {
        return Ok(None);
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("<init>", descriptor) => {
            let target = crate::descriptor_arguments(descriptor)
                .iter()
                .position(|parameter| parameter == "Ljava/lang/Runnable;")
                .and_then(|index| args.get(index))
                .cloned()
                .unwrap_or(Value::Ref(None));
            rt.fields.insert(field, target);
            Ok(None)
        }
        ("run", "()V") => match rt.fields.get(&field).cloned() {
            Some(Value::Ref(None)) => Ok(Some(Value::Void)),
            Some(target) => {
                run_runnable(rt, &target)?;
                Ok(Some(Value::Void))
            }
            None => Ok(None),
        },
        _ => Ok(None),
    }
}

fn invoke_future<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = state_marker(rt, receiver, HostKind::ExecutorFuture)? else {
        return Ok(None);
    };
    let value = rt
        .fields
        .get(&format!("{marker}|{FUTURE_VALUE}"))
        .cloned()
        .unwrap_or(Value::Ref(None));
    let failure = rt
        .fields
        .get(&format!("{marker}|{FUTURE_FAILURE}"))
        .cloned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("join", "()Ljava/lang/Object;") if failure.is_some() => {
            Err(completion_error(rt, failure.unwrap(), true)?)
        }
        ("get", "()Ljava/lang/Object;")
        | ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;")
            if failure.is_some() =>
        {
            Err(completion_error(rt, failure.unwrap(), false)?)
        }
        ("getNow", "(Ljava/lang/Object;)Ljava/lang/Object;") if failure.is_some() => {
            Err(completion_error(rt, failure.unwrap(), true)?)
        }
        ("get", "()Ljava/lang/Object;")
        | ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;")
        | ("join", "()Ljava/lang/Object;")
        | ("getNow", "(Ljava/lang/Object;)Ljava/lang/Object;") => Ok(Some(value)),
        ("isDone", "()Z") | ("isCompletedExceptionally", "()Z") if failure.is_some() => {
            Ok(Some(Value::Int(1)))
        }
        ("isDone", "()Z") => Ok(Some(Value::Int(1))),
        ("isCompletedExceptionally", "()Z") => Ok(Some(Value::Int(0))),
        ("isCancelled", "()Z") | ("cancel", "(Z)Z") => Ok(Some(Value::Int(0))),
        ("complete", "(Ljava/lang/Object;)Z")
        | ("completeExceptionally", "(Ljava/lang/Throwable;)Z") => Ok(Some(Value::Int(0))),
        ("toCompletableFuture", "()Ljava/util/concurrent/CompletableFuture;") => {
            Ok(receiver.cloned())
        }
        (
            "thenApply",
            "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;"
            | "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;",
        ) => {
            if let Some(failure) = failure.clone() {
                return Ok(Some(failed_future(rt, failure)));
            }
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            match invoke_stage_callback(rt, callback, &[value])? {
                Ok(Some(result)) => Ok(Some(complete_future(rt, result))),
                Ok(None) => Ok(None),
                Err(failure) => Ok(Some(failed_future(rt, failure))),
            }
        }
        (
            "thenAccept",
            "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletableFuture;"
            | "(Ljava/util/function/Consumer;)Ljava/util/concurrent/CompletionStage;",
        ) => {
            if let Some(failure) = failure.clone() {
                return Ok(Some(failed_future(rt, failure)));
            }
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            match invoke_stage_callback(rt, callback, &[value])? {
                Ok(Some(_)) => Ok(Some(complete_future(rt, Value::Ref(None)))),
                Ok(None) => Ok(None),
                Err(failure) => Ok(Some(failed_future(rt, failure))),
            }
        }
        (
            "thenCompose",
            "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;"
            | "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletionStage;",
        ) => {
            if let Some(failure) = failure.clone() {
                return Ok(Some(failed_future(rt, failure)));
            }
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            match invoke_stage_callback(rt, callback, &[value])? {
                Ok(Some(result))
                    if host_executors::allocation(&result, HostKind::ExecutorFuture).is_some() =>
                {
                    Ok(Some(result))
                }
                Ok(Some(_)) => Ok(None),
                Ok(None) => Ok(None),
                Err(failure) => Ok(Some(failed_future(rt, failure))),
            }
        }
        (
            "exceptionally",
            "(Ljava/util/function/Function;)Ljava/util/concurrent/CompletableFuture;",
        ) => {
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            if let Some(failure) = failure {
                match invoke_callback_capturing(rt, callback, &[failure])? {
                    Ok(Some(value)) => Ok(Some(complete_future(rt, value))),
                    Ok(None) => Ok(None),
                    Err(error) => Ok(Some(failed_future(rt, error))),
                }
            } else {
                Ok(Some(complete_future(rt, value)))
            }
        }
        ("handle", "(Ljava/util/function/BiFunction;)Ljava/util/concurrent/CompletableFuture;") => {
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            let throwable = failure.clone().unwrap_or(Value::Ref(None));
            match invoke_callback_capturing(rt, callback, &[value, throwable])? {
                Ok(Some(value)) => Ok(Some(complete_future(rt, value))),
                Ok(None) => Ok(None),
                Err(error) => Ok(Some(failed_future(rt, error))),
            }
        }
        (
            "whenComplete",
            "(Ljava/util/function/BiConsumer;)Ljava/util/concurrent/CompletableFuture;",
        ) => {
            let Some(callback) = args.first() else {
                return Err(native_npe());
            };
            let throwable = failure.clone().unwrap_or(Value::Ref(None));
            match invoke_callback_capturing(rt, callback, &[value.clone(), throwable])? {
                Ok(None) => Ok(None),
                Err(error) => {
                    if let Some(original) = failure.clone() {
                        if let Some(key) = jar_object::instance_key(&original) {
                            rt.host.record_throwable_suppressed(key, error);
                        }
                        Ok(Some(failed_future(rt, original)))
                    } else {
                        Ok(Some(failed_future(rt, error)))
                    }
                }
                Ok(Some(_)) => {
                    if let Some(failure) = failure {
                        Ok(Some(failed_future(rt, failure)))
                    } else {
                        Ok(Some(complete_future(rt, value)))
                    }
                }
            }
        }
        _ => Ok(None),
    }
}

fn invoke_stage_callback<H: Host>(
    rt: &mut Runtime<'_, H>,
    callback: &Value,
    args: &[Value],
) -> Result<Result<Option<Value>, Value>, NativeError> {
    match rt.invoke_callback(callback, args) {
        Ok(result) => Ok(Ok(result)),
        Err(NativeError::UncaughtException {
            class,
            message,
            frame,
        }) => Ok(Err(rt.caught_throwable(&class, message, frame.as_deref()))),
        Err(error) => Err(error),
    }
}

fn invoke_callback_capturing<H: Host>(
    rt: &mut Runtime<'_, H>,
    callback: &Value,
    args: &[Value],
) -> Result<Result<Option<Value>, Value>, NativeError> {
    match rt.invoke_callback(callback, args) {
        Ok(result) => Ok(Ok(result)),
        Err(NativeError::UncaughtException {
            class,
            message,
            frame,
        }) => Ok(Err(rt.caught_throwable(&class, message, frame.as_deref()))),
        Err(error) => Err(error),
    }
}

fn completion_error<H: Host>(
    rt: &mut Runtime<'_, H>,
    failure: Value,
    join: bool,
) -> Result<NativeError, NativeError> {
    let class = if join {
        "java/util/concurrent/CompletionException"
    } else {
        "java/util/concurrent/ExecutionException"
    };
    if join && rt.throwable_class(&failure) == class {
        return Ok(rt.throw_object(&failure, None));
    }
    let wrapper = rt.new_object(class);
    let init = MethodRef {
        owner: class.into(),
        name: "<init>".into(),
        descriptor: "(Ljava/lang/Throwable;)V".into(),
    };
    rt.host.invoke_instance(&init, &wrapper, &[failure])?;
    Ok(rt.throw_object(&wrapper, None))
}

fn reject_after_shutdown<H: Host>(rt: &Runtime<'_, H>, marker: &str) -> Result<(), NativeError> {
    if rt.executor_shutdown(marker) {
        return Err(NativeError::UncaughtException {
            class: "java/util/concurrent/RejectedExecutionException".into(),
            message: None,
            frame: None,
        });
    }
    Ok(())
}

/// A completed future holding `value`.
fn complete_future<H: Host>(rt: &mut Runtime<'_, H>, value: Value) -> Value {
    completed(rt, "future", value)
}

fn failed_future<H: Host>(rt: &mut Runtime<'_, H>, failure: Value) -> Value {
    let future = rt.new_object("future");
    if let Some(marker) = expect_ref(&future) {
        rt.fields
            .insert(format!("{marker}|{FUTURE_FAILURE}"), failure);
    }
    host_executors::allocated(HostKind::ExecutorFuture, future)
}

/// A completed `ScheduledFuture` holding `value`.
fn complete_scheduled_future<H: Host>(rt: &mut Runtime<'_, H>, value: Value) -> Value {
    completed(rt, "scheduled-future", value)
}

fn completed<H: Host>(rt: &mut Runtime<'_, H>, prefix: &str, value: Value) -> Value {
    let future = rt.new_object(prefix);
    if let Some(marker) = expect_ref(&future) {
        rt.fields.insert(format!("{marker}|{FUTURE_VALUE}"), value);
    }
    host_executors::allocated(HostKind::ExecutorFuture, future)
}

/// Run one `Runnable` value: a lambda marker runs its body, a jar object
/// dispatches its own `run`, and anything else is the host's call (counted).
fn run_runnable<H: Host>(rt: &mut Runtime<'_, H>, task: &Value) -> Result<(), NativeError> {
    if rt.invoke_callback(task, &[])?.is_some() {
        return Ok(());
    }
    // A handle that is no callback (an `Executors.callable` passed as a
    // `Runnable`) is the host's call: counted.
    if matches!(task, Value::Host(_)) {
        rt.host.invoke_instance(&run_ref(), task, &[])?;
        return Ok(());
    }
    if *task == Value::Ref(None) {
        return Err(native_npe());
    }
    if let Some(class) = crate::jar_object::object_class(task)
        && rt
            .class(class)
            .is_some_and(|file| file.method("run", RUNNABLE_RUN).is_some())
    {
        let class = class.to_owned();
        rt.invoke(&class, "run", RUNNABLE_RUN, Some(task.clone()), &[])?;
        return Ok(());
    }
    rt.host.invoke_instance(&run_ref(), task, &[])?;
    Ok(())
}

/// Run one `Callable` value and answer what it returned.
fn run_callable<H: Host>(rt: &mut Runtime<'_, H>, task: &Value) -> Result<Value, NativeError> {
    // `Executors.callable(task, result)`: run the task, answer the result.
    if let Some((task, result)) = host_executors::callable_parts(task) {
        run_runnable(rt, &task)?;
        return Ok(result.unwrap_or(Value::Ref(None)));
    }
    if let Some(value) = rt.invoke_callback(task, &[])? {
        return Ok(value);
    }
    if *task == Value::Ref(None) {
        return Err(native_npe());
    }
    if let Some(class) = crate::jar_object::object_class(task)
        && rt
            .class(class)
            .is_some_and(|file| file.method("call", CALLABLE_CALL).is_some())
    {
        let class = class.to_owned();
        return rt.invoke(&class, "call", CALLABLE_CALL, Some(task.clone()), &[]);
    }
    rt.host
        .invoke_instance(
            &MethodRef {
                owner: "java/util/concurrent/Callable".into(),
                name: "call".into(),
                descriptor: CALLABLE_CALL.into(),
            },
            task,
            &[],
        )
        .map(|value| {
            if value == Value::Void {
                Value::Ref(None)
            } else {
                value
            }
        })
}

impl<H: Host> Runtime<'_, H> {
    fn executor_shutdown(&self, marker: &str) -> bool {
        matches!(
            self.fields.get(&format!("{marker}|{SHUTDOWN_FIELD}")),
            Some(Value::Int(flag)) if *flag != 0
        )
    }

    fn set_executor_shutdown(&mut self, marker: &str) {
        self.fields
            .insert(format!("{marker}|{SHUTDOWN_FIELD}"), Value::Int(1));
    }
}

fn native_npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}
