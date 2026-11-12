//! Discord 頻道頁。

use slint::{ComponentHandle, ModelRc, VecModel};
use whatplus_proto::{Destination, Request, Response};

use crate::convert::{language_from_index, strings};
use crate::ctx::Ctx;
use crate::{AppWindow, DestData, DestinationRow};

pub fn init(ctx: &Ctx, ui: &AppWindow) {
    let data = ui.global::<DestData>();
    {
        let ctx = ctx.clone();
        data.on_refresh(move || refresh(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_load_channels(move || {
            ctx.call(Request::ListDiscordChannels, |ctx, ui, resp| {
                let Response::Guilds { guilds } = resp else {
                    return;
                };
                let d = ui.global::<DestData>();
                d.set_guilds(strings(guilds.iter().map(|g| g.name.clone())));
                d.set_guild_index(-1);
                d.set_channels(strings(Vec::new()));
                d.set_channel_index(-1);
                if guilds.is_empty() {
                    ctx.error(ctx.t(
                        "The bot has not joined any Discord server yet.",
                        "Bot 還沒有加入任何 Discord Server。",
                    ));
                }
                ctx.cache().guilds = guilds;
            });
        });
    }
    {
        let ctx = ctx.clone();
        data.on_guild_changed(move |i| {
            let Some(ui) = ctx.ui.upgrade() else { return };
            let cache = ctx.cache();
            let channels: Vec<String> = cache
                .guilds
                .get(i as usize)
                .map(|g| g.channels.iter().map(|c| format!("#{}", c.name)).collect())
                .unwrap_or_default();
            let d = ui.global::<DestData>();
            d.set_channels(strings(channels));
            d.set_channel_index(-1);
        });
    }
    {
        let ctx = ctx.clone();
        data.on_add(move || add(&ctx));
    }
    {
        let ctx = ctx.clone();
        data.on_test(move |id| {
            ctx.call(
                Request::TestDestination { id: id as i64 },
                |ctx, _, resp| {
                    if let Response::Sent { results } = resp {
                        let text = results.join("\n");
                        if text.contains("FAILED") {
                            ctx.error(text)
                        } else {
                            ctx.info(text)
                        }
                    }
                    refresh(ctx);
                },
            );
        });
    }
    {
        let ctx = ctx.clone();
        data.on_toggle(move |id| {
            let Some(mut d) = find(&ctx, id) else { return };
            d.enabled = !d.enabled;
            ctx.call(Request::SaveDestination { destination: d }, |ctx, _, _| {
                refresh(ctx)
            });
        });
    }
    {
        let ctx = ctx.clone();
        data.on_delete(move |id| {
            let Some(d) = find(&ctx, id) else { return };
            let question = format!("{} \"{}\"?", ctx.t("Delete channel", "刪除頻道"), d.name);
            if super::confirm(&ctx, &question) {
                ctx.call(Request::DeleteDestination { id: d.id }, |ctx, _, _| {
                    refresh(ctx)
                });
            }
        });
    }
}

fn find(ctx: &Ctx, id: i32) -> Option<Destination> {
    ctx.cache()
        .destinations
        .iter()
        .find(|d| d.id == id as i64)
        .cloned()
}

fn add(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else { return };
    let d = ui.global::<DestData>();
    let picked = {
        let cache = ctx.cache();
        cache
            .guilds
            .get(d.get_guild_index() as usize)
            .and_then(|g| {
                g.channels
                    .get(d.get_channel_index() as usize)
                    .map(|c| (g.id.clone(), g.name.clone(), c.id.clone(), c.name.clone()))
            })
    };
    let Some((guild_id, guild_name, channel_id, channel_name)) = picked else {
        ctx.error(ctx.t("Choose a server and a channel.", "請選擇 Server 與頻道。"));
        return;
    };
    let name = d.get_name().trim().to_string();
    let destination = Destination {
        id: 0,
        name: if name.is_empty() {
            format!("#{channel_name}")
        } else {
            name
        },
        guild_id,
        guild_name,
        channel_id,
        channel_name,
        language: language_from_index(d.get_language_index()),
        enabled: true,
        last_result: None,
    };
    ctx.call(Request::SaveDestination { destination }, |ctx, ui, _| {
        ui.global::<DestData>().set_name("".into());
        ctx.info(ctx.t(
            "Channel added. Use Test to check that the bot can post there.",
            "已新增頻道。可以按「測試」確認 Bot 能發訊息。",
        ));
        refresh(ctx);
    });
}

pub fn refresh(ctx: &Ctx) {
    ctx.call(Request::ListDestinations, |ctx, ui, resp| {
        let Response::Destinations { destinations } = resp else {
            return;
        };
        let rows: Vec<DestinationRow> = destinations
            .iter()
            .map(|d| DestinationRow {
                id: d.id as i32,
                name: d.name.clone().into(),
                channel: format!("{} / #{}", d.guild_name, d.channel_name).into(),
                language: d.language.code().into(),
                result: d.last_result.clone().unwrap_or_else(|| "-".into()).into(),
                enabled: d.enabled,
            })
            .collect();
        ui.global::<DestData>()
            .set_rows(ModelRc::new(VecModel::from(rows)));
        ctx.cache().destinations = destinations;
        super::push::update_destinations(ctx, ui);
        super::schedules::update_destinations(ctx, ui);
    });
}
