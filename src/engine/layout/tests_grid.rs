use super::*;

#[test]
fn places_explicit_items_across_fractional_and_fixed_tracks() {
    let page = Page::parse(
        r#"
            <style>
                body { margin: 0 }
                #container { display: flex }
                .grid { display: grid; width: 900px;
                        grid-template-columns: 1fr 1fr 300px }
                .main { grid-area: 1 / 1 / 2 / 3; height: 40px; background: #ff0000 }
                .side { grid-area: 1 / 3 / 2 / 4; height: 60px; background: #0000ff }
            </style>
            <div id="container"><div class="grid">
                <main class="main"></main><aside class="side"></aside>
            </div></div>
        "#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 900.0, 600.0, &mut measurer);
    let main = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    let side = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(0, 0, 255) => {
                Some(*rect)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        main,
        RectF {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 40.0
        }
    );
    assert_eq!(
        side,
        RectF {
            x: 600.0,
            y: 0.0,
            width: 300.0,
            height: 60.0
        }
    );
}

#[test]
fn places_named_areas_from_template_shorthand() {
    let page = Page::parse(
        r#"
            <style>
                body { margin: 0 }
                .grid { display: grid; width: 900px;
                        grid-template: min-content 1fr / 200px minmax(0, 1fr);
                        grid-template-areas: 'notice notice' 'sidebar content';
                        column-gap: 20px; row-gap: 5px }
                .notice { grid-area: notice; height: 10px; background: #ff0000 }
                .sidebar { grid-area: sidebar; height: 40px; background: #0000ff }
                .content { grid-area: content; height: 60px; background: #00ff00 }
            </style>
            <div class="grid"><header class="notice"></header>
                <aside class="sidebar"></aside><main class="content"></main></div>
        "#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 900.0, 600.0, &mut measurer);
    let rect_for = |wanted| {
        output.items.iter().find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if *color == wanted => Some(*rect),
            _ => None,
        })
    };

    assert_eq!(rect_for(Color::rgb(255, 0, 0)).unwrap().width, 900.0);
    assert_eq!(
        rect_for(Color::rgb(0, 0, 255)).unwrap(),
        RectF {
            x: 0.0,
            y: 15.0,
            width: 200.0,
            height: 40.0
        }
    );
    assert_eq!(
        rect_for(Color::rgb(0, 255, 0)).unwrap(),
        RectF {
            x: 220.0,
            y: 15.0,
            width: 680.0,
            height: 60.0
        }
    );
}
