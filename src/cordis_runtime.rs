use cordis::{
    BoxError, Context, Event, Plugin, PreparedPlugin, Routing,
    event::{ListenerRegistrationError, observer_sync},
};
use std::convert::Infallible;

struct HarnessLifecycle;
impl Event for HarnessLifecycle {
    const NAME: &'static str = "harness/lifecycle";
    type Args = String;
    type Output = ();
}
struct LifecycleLogger;
struct LifecycleInput;
impl Plugin for LifecycleLogger {
    type Config = ();
    type Input = LifecycleInput;
    type PrepareError = Infallible;
    type ApplyError = ListenerRegistrationError;

    fn prepare(&self, (): ()) -> Result<Self::Input, Self::PrepareError> {
        Ok(LifecycleInput)
    }

    async fn apply(&self, ctx: Context, _input: &Self::Input) -> Result<(), Self::ApplyError> {
        let _listener = ctx.on::<HarnessLifecycle, _>(observer_sync(|_, event| {
            tracing::debug!(event=%event, "Cordis lifecycle event");
            Ok::<_, Infallible>(())
        }))?;
        Ok(())
    }
}

pub struct CordisRuntime {
    ctx: Context,
    fiber: cordis::FiberHandle,
}
impl CordisRuntime {
    pub async fn start() -> Result<Self, BoxError> {
        let ctx = Context::new();
        let plugin = LifecycleLogger;
        let input = plugin.prepare(())?;
        let prepared = PreparedPlugin::from_input(plugin, input);
        let fiber = ctx.spawn(prepared).await?;
        ctx.emit::<HarnessLifecycle>(Routing::Unscoped, "started".to_owned())
            .await?;
        Ok(Self { ctx, fiber })
    }

    pub async fn emit(&self, event: impl Into<String>) -> Result<(), BoxError> {
        self.ctx
            .emit::<HarnessLifecycle>(Routing::Unscoped, event.into())
            .await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), BoxError> {
        self.fiber.dispose().await?;
        Ok(())
    }
}
