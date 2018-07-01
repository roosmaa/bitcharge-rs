use std::sync::Arc;
use gotham::handler::HandlerFuture;
use gotham::middleware::Middleware;
use gotham::state::State;

use cache::Caches;
use db::Database;
use harsh::Harsh;

#[derive(StateData)]
pub struct Env {
    pub db: Arc<Database>,
    pub caches: Arc<Caches>,
    pub hashids: Harsh,
}

#[derive(Clone, NewMiddleware)]
pub struct EnvMiddleware {
    pub db: Arc<Database>,
    pub caches: Arc<Caches>,
    pub hashids: Harsh,
}

impl Middleware for EnvMiddleware {
    fn call<Chain>(self, mut state: State, chain: Chain) -> Box<HandlerFuture>
        where Chain: FnOnce(State) -> Box<HandlerFuture>,
    {
        state.put(Env{
            db: self.db,
            caches: self.caches,
            hashids: self.hashids,
        });

        chain(state)
    }
}

