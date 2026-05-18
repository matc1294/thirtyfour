use crate::common::command::Command;
use crate::error::WebDriverResult;
use crate::session::handle::SessionHandle;
use crate::TypingData;

impl SessionHandle {
    /// Get the active alert text.
    ///
    /// # Example:
    /// ```no_run
    /// # use thirtyfour::prelude::*;
    /// # use thirtyfour::support::block_on;
    /// #
    /// # fn main() -> WebDriverResult<()> {
    /// #     block_on(async {
    /// #         let caps = DesiredCapabilities::chrome();
    /// #         let driver = WebDriver::new("http://localhost:4444", caps).await?;
    /// let text = driver.get_alert_text().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a `WebDriverError` if there is no active alert or the request fails.
    pub async fn get_alert_text(&self) -> WebDriverResult<String> {
        self.cmd(Command::GetAlertText).await?.value::<String>()
    }

    /// Dismiss the active alert.
    ///
    /// # Example:
    /// ```no_run
    /// # use thirtyfour::prelude::*;
    /// # use thirtyfour::support::block_on;
    /// #
    /// # fn main() -> WebDriverResult<()> {
    /// #     block_on(async {
    /// #         let caps = DesiredCapabilities::chrome();
    /// #         let driver = WebDriver::new("http://localhost:4444", caps).await?;
    /// driver.dismiss_alert().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a `WebDriverError` if there is no active alert or the request fails.
    pub async fn dismiss_alert(&self) -> WebDriverResult<()> {
        self.cmd(Command::DismissAlert).await?;
        Ok(())
    }

    /// Accept the active alert.
    ///
    /// # Example:
    /// ```no_run
    /// # use thirtyfour::prelude::*;
    /// # use thirtyfour::support::block_on;
    /// #
    /// # fn main() -> WebDriverResult<()> {
    /// #     block_on(async {
    /// #         let caps = DesiredCapabilities::chrome();
    /// #         let driver = WebDriver::new("http://localhost:4444", caps).await?;
    /// driver.accept_alert().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a `WebDriverError` if there is no active alert or the request fails.
    pub async fn accept_alert(&self) -> WebDriverResult<()> {
        self.cmd(Command::AcceptAlert).await?;
        Ok(())
    }

    /// Send the specified keys to the active alert.
    ///
    /// # Example:
    /// You can specify anything that implements `Into<TypingData>`. This
    /// includes &str and String.
    /// ```no_run
    /// # use thirtyfour::prelude::*;
    /// # use thirtyfour::support::block_on;
    /// #
    /// # fn main() -> WebDriverResult<()> {
    /// #     block_on(async {
    /// #         let caps = DesiredCapabilities::chrome();
    /// #         let driver = WebDriver::new("http://localhost:4444", caps).await?;
    /// driver.send_alert_text("selenium").await?;
    /// driver.accept_alert().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    ///
    /// You can also send special key combinations like this:
    /// ```no_run
    /// # use thirtyfour::prelude::*;
    /// # use thirtyfour::support::block_on;
    /// #
    /// # fn main() -> WebDriverResult<()> {
    /// #     block_on(async {
    /// #         let caps = DesiredCapabilities::chrome();
    /// #         let driver = WebDriver::new("http://localhost:4444", caps).await?;
    /// driver.send_alert_text(Key::Control + "a").await?;
    /// driver.send_alert_text("thirtyfour").await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a `WebDriverError` if there is no active alert or the request fails.
    pub async fn send_alert_text(&self, keys: impl Into<TypingData>) -> WebDriverResult<()> {
        self.cmd(Command::SendAlertText(keys.into())).await?;
        Ok(())
    }
}
