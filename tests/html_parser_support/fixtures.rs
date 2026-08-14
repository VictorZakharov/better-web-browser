use better_web_browser::engine::dom::{Node, NodeRef};

const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const MATHML_NAMESPACE: &str = "http://www.w3.org/1998/Math/MathML";

#[derive(Clone, Copy, Debug)]
enum ScriptingMode {
    Both,
    Enabled,
    Disabled,
}

#[derive(Debug)]
pub struct FragmentContext {
    namespace: &'static str,
    local_name: String,
}

impl FragmentContext {
    pub fn create_node(&self) -> NodeRef {
        if self.namespace == HTML_NAMESPACE {
            return Node::create_element(&self.local_name);
        }
        let owner = Node::create_element("html");
        Node::create_element_ns_for(&owner, self.namespace, &self.local_name)
    }
}

#[derive(Debug)]
pub struct Fixture {
    pub input: String,
    pub context: Option<FragmentContext>,
    pub expected: String,
    scripting: ScriptingMode,
}

impl Fixture {
    pub fn scripting_modes(&self) -> std::vec::IntoIter<bool> {
        match self.scripting {
            ScriptingMode::Both => vec![false, true],
            ScriptingMode::Enabled => vec![true],
            ScriptingMode::Disabled => vec![false],
        }
        .into_iter()
    }
}

pub fn parse_fixtures(source: &str) -> Result<Vec<Fixture>, String> {
    let normalized = source.replace("\r\n", "\n");
    let source = normalized
        .strip_prefix("#data\n")
        .ok_or("fixture suite must begin with #data")?;
    source
        .split("\n\n#data\n")
        .enumerate()
        .map(|(index, record)| parse_fixture(record, index + 1))
        .collect()
}

fn parse_fixture(record: &str, index: usize) -> Result<Fixture, String> {
    let (input, metadata) = split_marker(record, "#errors")
        .ok_or_else(|| format!("case {index} is missing #errors"))?;
    let (metadata, expected) = split_marker(metadata, "#document")
        .ok_or_else(|| format!("case {index} is missing #document"))?;
    let scripting = match (
        metadata.lines().any(|line| line == "#script-on"),
        metadata.lines().any(|line| line == "#script-off"),
    ) {
        (false, false) => ScriptingMode::Both,
        (true, false) => ScriptingMode::Enabled,
        (false, true) => ScriptingMode::Disabled,
        (true, true) => return Err(format!("case {index} enables and disables scripting")),
    };
    let context = metadata
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|lines| lines[0] == "#document-fragment")
        .map(|lines| parse_context(lines[1]))
        .transpose()?;

    let expected = expected.trim_end_matches('\n');
    Ok(Fixture {
        input: input.to_string(),
        context,
        expected: if expected.is_empty() {
            "#document".to_string()
        } else {
            format!("#document\n{expected}")
        },
        scripting,
    })
}

fn split_marker<'a>(source: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let at_start = source
        .strip_prefix(marker)
        .and_then(|rest| rest.strip_prefix('\n'))
        .map(|rest| ("", rest));
    at_start.or_else(|| {
        let marker = format!("\n{marker}\n");
        source.split_once(&marker)
    })
}

fn parse_context(source: &str) -> Result<FragmentContext, String> {
    let (namespace, local_name) = if let Some(local_name) = source.strip_prefix("svg ") {
        (SVG_NAMESPACE, local_name)
    } else if let Some(local_name) = source.strip_prefix("math ") {
        (MATHML_NAMESPACE, local_name)
    } else {
        (HTML_NAMESPACE, source)
    };
    if local_name.is_empty() {
        return Err("fragment context must have a local name".to_string());
    }
    Ok(FragmentContext {
        namespace,
        local_name: local_name.to_string(),
    })
}
