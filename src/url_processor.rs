use anyhow::Result;
use arboard::Clipboard;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Node, Selector};
use std::collections::HashSet;
use url::Url;

pub struct UrlProcessor {
    client: Client,
    max_depth: usize,
    visited: HashSet<String>,
}

impl UrlProcessor {
    pub fn new(max_depth: usize) -> Self {
        Self {
            client: Client::new(),
            max_depth,
            visited: HashSet::new(),
        }
    }

    pub fn process_url(&mut self, url: &str, traverse_links: bool) -> Result<String> {
        let url = Url::parse(url)?;
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Processing {}", url));

        let content = self.fetch_url(&url)?;
        let mut markdown = self.html_to_markdown(&content, &url);

        if traverse_links && self.max_depth > 0 {
            let links = self.extract_links(&content, &url)?;
            for link in links {
                if !self.visited.contains(&link) {
                    self.visited.insert(link.clone());
                    pb.set_message(format!("Processing sublink: {}", link));
                    let mut sub_processor = UrlProcessor::new(self.max_depth - 1);
                    if let Ok(sub_content) = sub_processor.process_url(&link, true) {
                        markdown.push_str("\n\n---\n\n");
                        markdown.push_str(&format!("## Content from {}\n\n", link));
                        markdown.push_str(&sub_content);
                    }
                }
            }
        }

        pb.finish_with_message(format!("Finished processing {}", url));

        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(&markdown);
        }

        Ok(markdown)
    }

    fn fetch_url(&self, url: &Url) -> Result<String> {
        Ok(self.client.get(url.as_str()).send()?.text()?)
    }

    fn html_to_markdown(&self, html: &str, base_url: &Url) -> String {
        let document = Html::parse_document(html);
        let body_selector = Selector::parse("body").unwrap();
        let mut markdown = String::new();

        if let Some(body) = document.select(&body_selector).next() {
            self.process_node(body, base_url, &mut markdown, 0);
        }

        markdown = markdown
            .replace("\n\n\n\n", "\n\n")
            .replace("\n\n\n", "\n\n")
            .trim()
            .to_string();

        markdown
    }

    fn process_node(&self, element: ElementRef, base_url: &Url, output: &mut String, depth: usize) {
        for node in element.children() {
            match node.value() {
                Node::Text(text) => {
                    let text = text.trim();
                    if !text.is_empty() {
                        output.push_str(text);
                        output.push(' ');
                    }
                }
                Node::Element(element) => match element.name() {
                    "p" => {
                        output.push_str("\n\n");
                        if let Some(child_ref) = ElementRef::wrap(node) {
                            self.process_node(child_ref, base_url, output, depth + 1);
                        }
                        output.push_str("\n");
                    }
                    "br" => output.push_str("\n"),
                    "h1" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "#")
                        }
                    }
                    "h2" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "##")
                        }
                    }
                    "h3" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "###")
                        }
                    }
                    "h4" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "####")
                        }
                    }
                    "h5" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "#####")
                        }
                    }
                    "h6" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_heading(el, base_url, output, "######")
                        }
                    }
                    "ul" | "ol" => {
                        if let Some(el) = ElementRef::wrap(node) {
                            self.process_list(el, base_url, output, depth)
                        }
                    }
                    "li" => {
                        output.push_str("\n");
                        output.push_str(&"  ".repeat(depth));
                        output.push_str("- ");
                        if let Some(child_ref) = ElementRef::wrap(node) {
                            self.process_node(child_ref, base_url, output, depth + 1);
                        }
                    }
                    "a" => {
                        let href = element
                            .attr("href")
                            .and_then(|href| base_url.join(href).ok())
                            .map(|url| url.to_string());

                        if let Some(href) = href {
                            let mut link_text = String::new();
                            if let Some(child_ref) = ElementRef::wrap(node) {
                                self.process_node(child_ref, base_url, &mut link_text, depth);
                            }
                            let link_text = link_text.trim();
                            if link_text.is_empty() {
                                output.push_str(&format!("[{}]({})", href, href));
                            } else {
                                output.push_str(&format!("[{}]({})", link_text, href));
                            }
                        }
                    }
                    "pre" | "code" => {
                        output.push_str("\n```\n");
                        if let Some(child_ref) = ElementRef::wrap(node) {
                            self.process_node(child_ref, base_url, output, depth);
                        }
                        output.push_str("\n```\n");
                    }
                    "blockquote" => {
                        output.push_str("\n> ");
                        if let Some(child_ref) = ElementRef::wrap(node) {
                            self.process_node(child_ref, base_url, output, depth);
                        }
                        output.push('\n');
                    }
                    _ => {
                        if let Some(child_ref) = ElementRef::wrap(node) {
                            self.process_node(child_ref, base_url, output, depth);
                        }
                    }
                },
                _ => {}
            }
        }
    }

    fn process_heading(
        &self,
        element: ElementRef,
        base_url: &Url,
        output: &mut String,
        level: &str,
    ) {
        output.push_str("\n\n");
        output.push_str(level);
        output.push(' ');
        self.process_node(element, base_url, output, 0);
        output.push_str("\n");
    }

    fn process_list(&self, element: ElementRef, base_url: &Url, output: &mut String, depth: usize) {
        output.push('\n');
        self.process_node(element, base_url, output, depth + 1);
        output.push('\n');
    }

    fn extract_links(&self, html: &str, base_url: &Url) -> Result<Vec<String>> {
        let document = Html::parse_document(html);
        let link_selector = Selector::parse("a[href]").unwrap();
        let mut links = Vec::new();

        for link in document.select(&link_selector) {
            if let Some(href) = link.value().attr("href") {
                if let Ok(absolute_url) = base_url.join(href) {
                    if absolute_url.scheme() == "http" || absolute_url.scheme() == "https" {
                        links.push(absolute_url.to_string());
                    }
                }
            }
        }

        Ok(links)
    }
}
