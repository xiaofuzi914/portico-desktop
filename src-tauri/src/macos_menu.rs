use tauri::{
    AppHandle, Wry,
    menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu},
};
const APP_NAME: &str = "Portico";

struct AppMenuLabels {
    about: &'static str,
    hide: &'static str,
    quit: &'static str,
}

fn app_menu_labels() -> AppMenuLabels {
    AppMenuLabels {
        about: "About Portico",
        hide: "Hide Portico",
        quit: "Quit Portico",
    }
}

pub(crate) fn build(app: &AppHandle<Wry>) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(app)?;
    let labels = app_menu_labels();
    let package_info = app.package_info();
    let config = app.config();
    let about_metadata = AboutMetadata {
        name: Some(APP_NAME.to_owned()),
        version: Some(package_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|publisher| vec![publisher]),
        ..Default::default()
    };

    let app_menu = Submenu::with_items(
        app,
        APP_NAME,
        true,
        &[
            &PredefinedMenuItem::about(app, Some(labels.about), Some(about_metadata))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(labels.hide))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(labels.quit))?,
        ],
    )?;

    let _ = menu.remove_at(0)?;
    menu.insert(&app_menu, 0)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::app_menu_labels;

    #[test]
    fn app_menu_uses_the_product_name_instead_of_the_crate_name() {
        let labels = app_menu_labels();

        assert_eq!(labels.about, "About Portico");
        assert_eq!(labels.hide, "Hide Portico");
        assert_eq!(labels.quit, "Quit Portico");
        assert!(!labels.about.contains("tauri"));
        assert!(!labels.hide.contains("tauri"));
        assert!(!labels.quit.contains("tauri"));
    }
}
