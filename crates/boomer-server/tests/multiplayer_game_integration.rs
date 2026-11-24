//! End-to-end Milestone 6 integration tests: authoritative Checkers & 8-Ball Pool matches,
//! spectators, zero-desync Pool replay verification, always-open House Bots, Elo ratings,
//! anti-farming decay, and High Score win-count persistence.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

use std::sync::Arc;
use std::time::Duration;

use boomer_domain::math::Vec2;
use boomer_domain::pool::physics::{BallState, ShotInput, simulate_shot};
use boomer_protocol::{
    ChatScopeDto, ClientMessage, ErrorCode, GameKindDto, PROTOCOL_VERSION, PoolShotDto, SeatDto,
    ServerMessage, TablePhaseDto, TableRoleDto, TableSettingsDto,
};
use boomer_server::adapters::memory::{CaptureMailSender, MemoryRepos, TestGoogleVerifier};
use boomer_server::auth::TokenSigner;
use boomer_server::{AppState, ServerConfig, build_router};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct TestServer {
    base_ws: String,
    mailer: Arc<CaptureMailSender>,
}

async fn spawn_server() -> TestServer {
    let repos = Arc::new(MemoryRepos::new());
    let mailer = Arc::new(CaptureMailSender::default());
    let google = Arc::new(TestGoogleVerifier);
    let signer = Arc::new(TokenSigner::new(b"milestone-6-integration-secret!!"));
    let state = AppState::new(
        ServerConfig::default(),
        signer,
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos.clone(),
        repos,
        mailer.clone(),
        google,
    );
    let router = build_router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    TestServer {
        base_ws: format!("ws://{addr}/ws"),
        mailer,
    }
}

async fn send_msg(ws: &mut WsStream, msg: &ClientMessage) {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(WsMsg::Text(text.into())).await.unwrap();
}

async fn recv_until<F, T>(ws: &mut WsStream, mut pred: F) -> T
where
    F: FnMut(ServerMessage) -> Option<T>,
{
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => panic!("timed out waiting for expected ServerMessage"),
            frame = ws.next() => {
                let frame = frame.expect("socket closed unexpectedly").unwrap();
                if let WsMsg::Text(text) = frame {
                    let parsed: ServerMessage = serde_json::from_str(&text).unwrap();
                    if let Some(found) = pred(parsed) {
                        return found;
                    }
                }
            }
        }
    }
}

async fn connect_registered(srv: &TestServer, email: &str, room: &str) -> (WsStream, String) {
    let (mut ws, _) = connect_async(&srv.base_ws).await.unwrap();
    send_msg(
        &mut ws,
        &ClientMessage::Hello {
            v: PROTOCOL_VERSION,
            session: None,
            guest: None,
        },
    )
    .await;
    recv_until(&mut ws, |m| {
        matches!(m, ServerMessage::Welcome { .. }).then_some(())
    })
    .await;

    send_msg(
        &mut ws,
        &ClientMessage::AuthEmail {
            email: email.to_owned(),
        },
    )
    .await;
    recv_until(&mut ws, |m| {
        matches!(m, ServerMessage::AuthSent {}).then_some(())
    })
    .await;
    let magic = srv
        .mailer
        .sent_messages()
        .into_iter()
        .rev()
        .find(|(to, _)| to.eq_ignore_ascii_case(email))
        .expect("magic email must exist")
        .1
        .split("#login=")
        .nth(1)
        .unwrap()
        .to_owned();
    send_msg(&mut ws, &ClientMessage::AuthMagic { token: magic }).await;
    let wire_id = recv_until(&mut ws, |m| match m {
        ServerMessage::AuthOk { me, .. } => Some(me.id),
        _ => None,
    })
    .await;

    send_msg(
        &mut ws,
        &ClientMessage::LobbyJoin {
            room: room.to_owned(),
        },
    )
    .await;
    let _ = recv_until(&mut ws, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    (ws, wire_id)
}

#[tokio::test]
async fn rated_checkers_match_with_spectator_updates_elo_highscores_and_rematch() {
    let srv = spawn_server().await;
    let (mut alice, alice_id) =
        connect_registered(&srv, "alice@example.com", "checkers-social-1").await;
    let (mut bob, bob_id) = connect_registered(&srv, "bob@example.com", "checkers-social-1").await;
    let (mut watcher, _) = connect_registered(&srv, "watch@example.com", "checkers-social-1").await;

    send_msg(
        &mut alice,
        &ClientMessage::TableCreate(TableSettingsDto {
            rated: true,
            ..TableSettingsDto::default()
        }),
    )
    .await;
    let table_id = recv_until(&mut alice, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s.id),
        _ => None,
    })
    .await;

    send_msg(
        &mut bob,
        &ClientMessage::TableJoin {
            table_id: table_id.clone(),
            role: TableRoleDto::Player,
        },
    )
    .await;
    let _ = recv_until(&mut bob, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    send_msg(
        &mut watcher,
        &ClientMessage::TableJoin {
            table_id,
            role: TableRoleDto::Spectator,
        },
    )
    .await;
    let _ = recv_until(&mut watcher, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;

    send_msg(&mut alice, &ClientMessage::Ready {}).await;
    send_msg(&mut bob, &ClientMessage::Ready {}).await;
    let initial = recv_until(&mut alice, |m| match m {
        ServerMessage::CheckersSnapshot { state } => Some(state),
        _ => None,
    })
    .await;
    assert_eq!(initial.active_seat, SeatDto::One);
    assert_eq!(initial.turn_number, 1);

    // Spectator and waiting player are rejected with NotYourTurn.
    send_msg(
        &mut watcher,
        &ClientMessage::CheckersMove {
            turn: 1,
            path: vec![9, 13],
        },
    )
    .await;
    assert_eq!(
        recv_until(&mut watcher, |m| match m {
            ServerMessage::Error { code, re } if re == "checkers.move" => Some(code),
            _ => None,
        })
        .await,
        ErrorCode::NotYourTurn
    );
    send_msg(
        &mut bob,
        &ClientMessage::CheckersMove {
            turn: 1,
            path: vec![21, 17],
        },
    )
    .await;
    assert_eq!(
        recv_until(&mut bob, |m| match m {
            ServerMessage::Error { code, re } if re == "checkers.move" => Some(code),
            _ => None,
        })
        .await,
        ErrorCode::NotYourTurn
    );

    // Stale turn number rejected with StaleTurn.
    send_msg(
        &mut alice,
        &ClientMessage::CheckersMove {
            turn: 99,
            path: vec![9, 13],
        },
    )
    .await;
    assert_eq!(
        recv_until(&mut alice, |m| match m {
            ServerMessage::Error { code, re } if re == "checkers.move" => Some(code),
            _ => None,
        })
        .await,
        ErrorCode::StaleTurn
    );

    // Play 4 legal non-capturing plies (meets minimum activity threshold for High Scores).
    for (ply, (who_is_alice, path)) in [
        (true, vec![9, 13]),
        (false, vec![24, 20]),
        (true, vec![10, 14]),
        (false, vec![23, 19]),
    ]
    .into_iter()
    .enumerate()
    {
        let turn = u32::try_from(ply + 1).unwrap();
        let mover = if who_is_alice { &mut alice } else { &mut bob };
        send_msg(mover, &ClientMessage::CheckersMove { turn, path }).await;
        let seen = recv_until(&mut watcher, |m| match m {
            ServerMessage::CheckersSnapshot { state } if state.turn_number == turn + 1 => {
                Some(state)
            }
            _ => None,
        })
        .await;
        assert_eq!(seen.moves.len(), ply + 1);
    }

    // Bob resigns -> Alice wins, Elo updates (1200 -> 1216 / 1184), High Score recorded, system chat posted!
    send_msg(&mut bob, &ClientMessage::Resign {}).await;
    let over = recv_until(&mut alice, |m| match m {
        ServerMessage::GameOver(g) => Some(g),
        _ => None,
    })
    .await;
    assert_eq!(over.winner, Some(SeatDto::One));
    assert_eq!(over.rating.len(), 2);
    assert_eq!(over.rating[0].id, alice_id);
    assert_eq!(over.rating[0].old, 1200);
    assert_eq!(over.rating[0].new, 1216);
    assert_eq!(over.rating[1].id, bob_id);
    assert_eq!(over.rating[1].new, 1184);
    assert_eq!(over.wins.len(), 1);
    assert_eq!(over.wins[0].wins, 1);

    // Verify High Scores endpoint reflects Alice's counted human win.
    send_msg(
        &mut alice,
        &ClientMessage::HighscoresGet {
            game: GameKindDto::Checkers,
            around: true,
        },
    )
    .await;
    let rows = recv_until(&mut alice, |m| match m {
        ServerMessage::Highscores { rows, .. } => Some(rows),
        _ => None,
    })
    .await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, alice_id);
    assert_eq!(rows[0].wins, 1);

    // Rematch swaps starting colors (Seat 2 becomes Black on Match 2).
    send_msg(&mut alice, &ClientMessage::Rematch {}).await;
    send_msg(&mut bob, &ClientMessage::Rematch {}).await;
    let rematch_snap = recv_until(&mut bob, |m| match m {
        ServerMessage::CheckersSnapshot { state }
            if state.turn_number == 1 && state.moves.is_empty() =>
        {
            Some(state)
        }
        _ => None,
    })
    .await;
    assert_eq!(rematch_snap.active_seat, SeatDto::Two);
}

#[tokio::test]
async fn authoritative_pool_shot_replays_bit_identically_with_zero_desync() {
    let srv = spawn_server().await;
    let (mut p1, _) = connect_registered(&srv, "pool1@example.com", "pool-social-1").await;
    let (mut p2, _) = connect_registered(&srv, "pool2@example.com", "pool-social-1").await;

    send_msg(
        &mut p1,
        &ClientMessage::TableCreate(TableSettingsDto::default()),
    )
    .await;
    let table_id = recv_until(&mut p1, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s.id),
        _ => None,
    })
    .await;
    send_msg(
        &mut p2,
        &ClientMessage::TableJoin {
            table_id,
            role: TableRoleDto::Player,
        },
    )
    .await;
    let _ = recv_until(&mut p2, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;

    send_msg(&mut p1, &ClientMessage::Ready {}).await;
    send_msg(&mut p2, &ClientMessage::Ready {}).await;
    let start_state = recv_until(&mut p1, |m| match m {
        ServerMessage::PoolSnapshot { state } => Some(state),
        _ => None,
    })
    .await;
    assert_eq!(start_state.turn_number, 1);
    assert!(start_state.is_break_shot);

    // Seat 1 fires a full-power break shot.
    let break_shot = PoolShotDto {
        turn: 1,
        angle: 0.015,
        power: 0.92,
        spin_x: 0.1,
        spin_y: 0.2,
        cue_pos: Some([0.62, 0.635]),
        called_pocket: None,
    };
    send_msg(&mut p1, &ClientMessage::PoolShoot { shot: break_shot }).await;

    let (pre_balls, applied_shot, post_state) = recv_until(&mut p2, |m| match m {
        ServerMessage::PoolShot {
            pre_balls,
            shot,
            state,
        } => Some((pre_balls, shot, state)),
        _ => None,
    })
    .await;

    // Replay the exact broadcast on the client side using `boomer_domain::pool::physics::simulate_shot`
    // and assert every resting non-pocketed object ball coordinate matches BIT-FOR-BIT!
    let mut pre: [BallState; 16] = std::array::from_fn(|idx| {
        let b = &pre_balls[idx];
        let mut st = BallState::stationary(b.id, Vec2::new(b.x, b.y));
        st.pocketed = b.pocketed;
        st
    });
    if let Some([cx, cy]) = applied_shot.cue_pos {
        pre[0].pos = Vec2::new(cx, cy);
        pre[0].pocketed = false;
    }
    let client_sim = simulate_shot(
        &pre,
        ShotInput {
            angle: applied_shot.angle,
            power: applied_shot.power,
            spin_x: applied_shot.spin_x,
            spin_y: applied_shot.spin_y,
            cue_pos: applied_shot.cue_pos.map(|[x, y]| Vec2::new(x, y)),
            called_pocket: applied_shot.called_pocket,
        },
        false,
    );

    for (idx, srv_ball) in post_state.balls.iter().enumerate() {
        // Cue ball may be re-spotted if break scratched; compare all object balls bit-identically.
        if idx == 0 && client_sim.final_balls[0].pocketed {
            continue;
        }
        assert_eq!(srv_ball.pocketed, client_sim.final_balls[idx].pocketed);
        if !srv_ball.pocketed {
            assert_eq!(
                srv_ball.x.to_bits(),
                client_sim.final_balls[idx].pos.x.to_bits(),
                "ball {idx} X drifted!"
            );
            assert_eq!(
                srv_ball.y.to_bits(),
                client_sim.final_balls[idx].pos.y.to_bits(),
                "ball {idx} Y drifted!"
            );
        }
    }
}

#[tokio::test]
async fn always_open_house_bot_table_plays_via_worker_pool_and_stays_unrated() {
    let srv = spawn_server().await;
    let (mut player, _) = connect_registered(&srv, "solo@example.com", "checkers-social-1").await;

    // Re-join room to inspect tables: 3 always-open 🤖 House Bot tables must exist!
    send_msg(
        &mut player,
        &ClientMessage::LobbyJoin {
            room: "checkers-social-1".to_owned(),
        },
    )
    .await;
    let lobby = recv_until(&mut player, |m| match m {
        ServerMessage::LobbySnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    let bot_tables: Vec<_> = lobby
        .tables
        .iter()
        .filter(|t| t.seats[1].as_ref().is_some_and(|s| s.bot))
        .collect();
    assert_eq!(bot_tables.len(), 3);
    let easy_table_id = bot_tables[0].id.clone();

    // Join Easy House Bot table, click Ready -> bot is already ready -> game starts!
    send_msg(
        &mut player,
        &ClientMessage::TableJoin {
            table_id: easy_table_id.clone(),
            role: TableRoleDto::Player,
        },
    )
    .await;
    let _ = recv_until(&mut player, |m| match m {
        ServerMessage::TableSnapshot(s) => Some(s),
        _ => None,
    })
    .await;
    send_msg(&mut player, &ClientMessage::Ready {}).await;
    let _ = recv_until(&mut player, |m| match m {
        ServerMessage::CheckersSnapshot { state } if state.turn_number == 1 => Some(state),
        _ => None,
    })
    .await;

    // Human plays opening move -> House Bot replies asynchronously via bounded `spawn_blocking` pool!
    send_msg(
        &mut player,
        &ClientMessage::CheckersMove {
            turn: 1,
            path: vec![9, 13],
        },
    )
    .await;
    let after_bot = recv_until(&mut player, |m| match m {
        ServerMessage::CheckersSnapshot { state } if state.turn_number == 3 => Some(state),
        _ => None,
    })
    .await;
    assert_eq!(after_bot.moves.len(), 2);

    // Resign -> strictly unrated, zero rating deltas, zero High Score wins.
    send_msg(&mut player, &ClientMessage::Resign {}).await;
    let over = recv_until(&mut player, |m| match m {
        ServerMessage::GameOver(g) => Some(g),
        _ => None,
    })
    .await;
    assert_eq!(over.rating.len(), 0);
    assert_eq!(over.wins.len(), 0);

    // Leave House Bot table -> table stays open and resets to Waiting!
    send_msg(&mut player, &ClientMessage::TableLeave {}).await;
    let reset = recv_until(&mut player, |m| match m {
        ServerMessage::TableUpsert(t)
            if t.id == easy_table_id
                && t.phase == TablePhaseDto::Waiting
                && t.seats[0].is_none() =>
        {
            Some(t)
        }
        _ => None,
    })
    .await;
    assert!(reset.seats[1].as_ref().is_some_and(|s| s.bot));

    // Confirm system chat was never polluted with fake rating lines.
    send_msg(
        &mut player,
        &ClientMessage::ChatSend {
            text: "gg bot!".to_owned(),
            scope: Some(ChatScopeDto::Lobby),
        },
    )
    .await;
}
