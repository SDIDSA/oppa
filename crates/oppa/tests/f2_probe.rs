//! F2 characterization regression test (retained deliberately).
//! The todo-app event pattern headlessly: text feeds + structural
//! adds interleaved with row presses, 200 rounds, every press
//! asserted against the shared signal. This file is the evidence
//! that exonerated framework dispatch in the F2 episode (200/200);
//! any future failure here reopens a genuine dispatch bug.

#![allow(non_snake_case)]

use oppa::{
    Color, Column, ComponentHost, Ctx, InputEvent, SharedString, Style, Text, TextField, VNode,
};
use oppa_macros::{component, Props};

#[derive(Clone)]
struct Item {
    label: SharedString,
    done: bool,
}

#[derive(Clone, Props)]
struct RowProps {
    index: usize,
    label: SharedString,
    todos: oppa::Signal<Vec<Item>>,
}

#[component]
fn ProbeRow(ctx: &Ctx, props: &RowProps) -> VNode {
    let _ = ctx;
    let todos = props.todos.clone();
    let idx = props.index;
    oppa::Div("row")
        .style(Style::new().size(300, 32).bg(Color(0x55_55_55)))
        .on_press(move || {
            todos.update(|mut items| {
                items[idx].done = !items[idx].done;
                items
            });
        })
        .child(
            Text {
                text: props.label.clone(),
                style: Text::title_small,
            }
            .into(),
        )
}

#[derive(Clone, Props)]
struct AppProps {
    field: oppa::Signal<SharedString>,
    todos: oppa::Signal<Vec<Item>>,
}

#[component]
fn ProbeApp(ctx: &Ctx, props: &AppProps) -> VNode {
    let todos = props.todos.clone();
    let add = todos.clone();
    let field_add = props.field.clone();
    let field_value = props.field.clone();
    let rows: Vec<VNode> = todos
        .get()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            ctx.child(
                "row",
                i as u64,
                &RowProps {
                    index: i,
                    label: item.label.clone(),
                    todos: todos.clone(),
                },
                ProbeRow,
            )
        })
        .collect();
    let mut kids: Vec<VNode> = vec![
        TextField {
            text: field_value.get(),
            style: Text::title_small,
            label: SharedString::from("New item"),
        }
        .into(),
        oppa::Div("add")
            .style(Style::new().size(120, 32).bg(Color(0x00_77_CC)))
            .on_press(move || {
                let label = field_add.get().to_string();
                add.update(|mut items| {
                    items.push(Item {
                        label: SharedString::from(label.clone()),
                        done: false,
                    });
                    items
                });
                field_add.set(SharedString::from(""));
            })
            .child(
                Text {
                    text: SharedString::from("Add item"),
                    style: Text::title_small,
                }
                .into(),
            ),
    ];
    kids.extend(rows);
    oppa::Div("screen")
        .style(Style::new().size(800, 600).bg(Color(0xFF_FF_FF)))
        .child(Column::new().children(kids))
}

fn press(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::pointer_down(x, y));
    host.inject_input(InputEvent::pointer_up(x, y));
    host.run_until_idle();
}

fn center_of(host: &ComponentHost, debug: &str, nth: usize) -> (f32, f32) {
    let ids = oppa::find_retained_by_debug(host, debug);
    assert!(!ids.is_empty(), "no {debug} nodes");
    let b = host
        .committed_box(ids[nth % ids.len()])
        .expect("committed box");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

fn seed() -> Vec<Item> {
    vec![
        Item {
            label: SharedString::from("Buy milk"),
            done: false,
        },
        Item {
            label: SharedString::from("Write spec"),
            done: true,
        },
    ]
}

#[test]
fn f2_press_after_swap_history() {
    for round in 0..200 {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let field = host.runtime().signal(SharedString::from(""));
        let todos = host.runtime().signal(seed());
        host.mount(
            "ProbeApp",
            AppProps {
                field: field.clone(),
                todos: todos.clone(),
            },
            ProbeApp,
        );
        host.run_until_idle();
        let fields = host.text_fields();
        assert_eq!(fields.len(), 1, "round {round}: one field");
        host.bind_text(fields[0], field.clone());
        // Keystroke history: every keystroke re-renders everything.
        for (k, ch) in "Buy eggs".chars().enumerate() {
            let cur = field.get().to_string() + &ch.to_string();
            host.inject_input(InputEvent::text(fields[0], cur.clone()));
            host.run_until_idle();
            assert_eq!(
                field.get().to_string(),
                cur,
                "round {round} key {k}: feed exact"
            );
        }
        // Add the typed row + 5 blank rows (structural churn).
        for _ in 0..6 {
            let (ax, ay) = center_of(&host, "add", 0);
            press(&host, ax, ay);
        }
        assert_eq!(todos.get().len(), 8, "round {round}: 8 rows");
        // Toggle row 0: exactly Buy milk flips false->true.
        let (rx, ry) = center_of(&host, "row", 0);
        press(&host, rx, ry);
        let items = todos.get();
        assert!(items[0].done, "round {round}: row 0 flipped");
        assert_eq!(
            items.iter().filter(|it| it.done).count(),
            2,
            "round {round}: exactly Write spec + row 0 done"
        );
        // Toggle row 0 back: count returns to 1.
        let (rx, ry) = center_of(&host, "row", 0);
        press(&host, rx, ry);
        let items = todos.get();
        assert!(!items[0].done, "round {round}: row 0 flipped back");
        assert_eq!(
            items.iter().filter(|it| it.done).count(),
            1,
            "round {round}: only Write spec done"
        );
    }
}
