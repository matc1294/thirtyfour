use crate::common::command::Command;
use crate::error::WebDriverErrorInfo;
use crate::session::handle::SessionHandle;
use crate::WindowHandle;
use crate::{
    error::{WebDriverError, WebDriverResult},
    WebElement,
};
use std::sync::Arc;

impl SessionHandle {
    /// Return the element with focus, or the `<body>` element if nothing has focus.
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
    /// // If no element has focus, active_element() will return the body tag.
    /// let active_elem = driver.active_element().await?;
    /// assert_eq!(active_elem.tag_name().await?, "body");
    ///
    /// // Now let's manually focus an element and try active_element() again.
    /// let elem = driver.find(By::Id("my-element-id")).await?;
    /// elem.focus().await?;
    ///
    /// // And fetch the active element again.
    /// let active_elem = driver.active_element().await?;
    /// assert_eq!(active_elem.element_id(), elem.element_id());
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn active_element(self: &Arc<SessionHandle>) -> WebDriverResult<WebElement> {
        let r = self.cmd(Command::GetActiveElement).await?;
        r.element(self.clone())
    }

    /// Switch to the default frame.
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
    /// // Enter the first iframe.
    /// driver.enter_frame(0).await?;
    /// // We are now inside the iframe.
    /// driver.find(By::Id("button1")).await?;
    /// driver.enter_default_frame().await?;
    /// // We are now back in the original window.
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn enter_default_frame(&self) -> WebDriverResult<()> {
        self.cmd(Command::SwitchToFrameDefault).await?;
        Ok(())
    }

    /// Switch to an iframe by index. The first iframe on the page has index 0.
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
    /// // Enter the first iframe.
    /// driver.enter_frame(0).await?;
    /// // We can now search for elements within the iframe.
    /// let elem = driver.find(By::Id("button1")).await?;
    /// elem.click().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn enter_frame(&self, frame_number: u16) -> WebDriverResult<()> {
        self.cmd(Command::SwitchToFrameNumber(frame_number)).await?;
        Ok(())
    }

    /// Switch to the parent frame.
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
    /// // Find the iframe element and enter the iframe.
    /// let elem_iframe = driver.find(By::Id("iframeid1")).await?;
    /// elem_iframe.enter_frame().await?;
    /// // We can now search for elements within the iframe.
    /// let elem = driver.find(By::Id("button1")).await?;
    /// elem.click().await?;
    /// // Now switch back to the parent frame.
    /// driver.enter_parent_frame().await?;
    /// // We are now back in the parent document.
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn enter_parent_frame(&self) -> WebDriverResult<()> {
        self.cmd(Command::SwitchToParentFrame).await?;
        Ok(())
    }

    /// Switch to the specified window.
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
    /// // Open a new tab.
    /// driver.new_tab().await?;
    ///
    /// // Get window handles and switch to the new tab.
    /// let handles = driver.windows().await?;
    /// driver.switch_to_window(handles[1].clone()).await?;
    ///
    /// // We are now controlling the new tab.
    /// driver.goto("https://www.rust-lang.org").await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn switch_to_window(&self, handle: WindowHandle) -> WebDriverResult<()> {
        self.cmd(Command::SwitchToWindow(handle)).await?;
        Ok(())
    }

    /// Switch to the window with the specified name. This uses the `window.name` property.
    /// You can set a window name via `WebDriver::set_window_name("someName").await?`.
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
    /// // Set main window name so we can switch back easily.
    /// driver.set_window_name("mywindow").await?;
    ///
    /// // Open a new tab.
    /// driver.new_tab().await?;
    ///
    /// // Get window handles and switch to the new tab.
    /// let handles = driver.windows().await?;
    /// driver.switch_to_window(handles[1].clone()).await?;
    ///
    /// // We are now controlling the new tab.
    /// assert_eq!(driver.title().await?, "");
    /// driver.switch_to_named_window("mywindow").await?;
    ///
    /// // We are now back in the original tab.
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn switch_to_named_window(
        self: &Arc<SessionHandle>,
        name: &str,
    ) -> WebDriverResult<()> {
        let original_handle = self.window().await?;
        let handles = self.windows().await?;
        for handle in handles {
            self.switch_to_window(handle).await?;
            let ret = self.execute(r"return window.name;", Vec::new()).await?;
            let current_name: String = ret.convert()?;
            if current_name == name {
                return Ok(());
            }
        }

        self.switch_to_window(original_handle).await?;
        Err(WebDriverError::NoSuchWindow(WebDriverErrorInfo::new(format!(
            "unable to find named window: {name}"
        ))))
    }

    /// Switch to a new window.
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
    /// // Open a new window.
    /// let handle = driver.new_window().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn new_window(&self) -> WebDriverResult<WindowHandle> {
        self.cmd(Command::NewWindow).await?.value()
    }

    /// Switch to a new tab.
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
    /// // Open a new tab in the current window.
    /// let handle = driver.new_tab().await?;
    /// #         driver.quit().await?;
    /// #         Ok(())
    /// #     })
    /// # }
    /// ```
    pub async fn new_tab(&self) -> WebDriverResult<WindowHandle> {
        self.cmd(Command::NewTab).await?.value()
    }
}
