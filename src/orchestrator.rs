use carboxyl::Stream;
use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableGraph;
use petgraph::Direction;

use crate::component::*;
use crate::component_instance::*;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

// TODO: Add threadpool concurrency via rayon crate (https://docs.rs/rayon/)
// exellent summary of various crates at https://www.reddit.com/r/rust/comments/djzd5t/which_asyncconcurrency_crate_to_choose_from/

// TODO: Add error handling via anyhow crate (https://docs.rs/anyhow/)
// summary of error handling at https://www.reddit.com/r/rust/comments/gqe57x/what_are_you_using_for_error_handling/
// anyhow for applications, thiserror for libraries (thiserror helps to not expose internal error handling to users)

/// connector connection connector
/// ------<in to----from out<-----
/// ----->out from----to in>------

// #[derive(Debug, Clone, PartialEq, Hash)]
// struct Connection {
//   pub owning_instance_ref: ComponentInstanceRef,
//   pub from_connector_ref: NodeRef,
//   pub to_connector_ref: NodeRef,
// }

pub type InstanceGraph = StableGraph<InstanceGraphNode, InstanceConnection>;

#[derive(Debug, Clone)]
pub struct InstanceConnection {
  from_connector_index: NodeIndex,
  to_connector_index: NodeIndex,
}

//todo: replace SignalRequest with proper signal system
// https://docs.rs/carboxyl/latest/carboxyl/
// https://crates.io/crates/event-listener-primitives

#[derive(Debug, Clone)]
pub struct InstanceConnectorRef {
  pub instance_ix: NodeIndex,
  pub connector_ix: NodeIndex,
}

pub struct OrchestratorData {
  components: HashMap<String, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  active_instance_ixs: HashSet<NodeIndex>,
  clock_cycle: usize,
  // keep track of all connections between component instances
  pub(crate) instance_graph: Rc<RefCell<InstanceGraph>>,
  root_instance_ref: Option<Rc<RefCell<InstanceRefNode>>>,
  instance_active_stream: Rc<RefCell<Stream<(NodeIndex, bool)>>>,
  signal_connector_stream: Rc<RefCell<Stream<InstanceConnectorRef>>>,
}

impl fmt::Debug for OrchestratorData {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(
      f,
      "OrchestratorData {{
  components: {:?}
  active_instance_ixs: {:?}
  clock_cycle: {}
  instance_graph: {:?}
  root_instance_ref: {:?}
  instance_active_stream: Rc<RefCell<Stream<(NodeIndex, bool)>>>
  signal_connector_stream: Rc<RefCell<Stream<InstanceConnectorRef>>
}}",
      self.components,
      self.active_instance_ixs,
      self.clock_cycle,
      self.instance_graph,
      self.root_instance_ref,
    )
  }
}

impl OrchestratorData {
  pub fn new() -> OrchestratorData {
    OrchestratorData {
      components: HashMap::new(),
      active_instance_ixs: HashSet::new(),
      clock_cycle: 0,
      instance_graph: Rc::new(RefCell::new(StableGraph::new())),
      root_instance_ref: None,
      instance_active_stream: Rc::new(RefCell::new(Stream::never())),
      signal_connector_stream: Rc::new(RefCell::new(Stream::never())),
    }
  }

  pub fn add_component(&mut self, component: Component) {
    self.components.insert(component.name.clone(), component);
  }

  pub fn add_root_component(&mut self, component: Component) -> &mut Self {
    self.root_instance_ref = Some(Rc::new(RefCell::new(InstanceRefNode {
      node_name: "Root".to_string(),
      component_name: component.name.clone(),
      instance_ix: None,
    })));
    self.components.insert(component.name.clone(), component);
    self
  }
}

#[derive(Debug)]
pub struct Orchestrator<'a> {
  data: &'a mut OrchestratorData,
}

impl<'a> Orchestrator<'a> {
  // pub fn new(orchestrator_data: &'a mut OrchestratorData) -> Self {
  //   orchestrator_data.as_orchestrator()
  // }

  pub fn new(data: &'a mut OrchestratorData) -> Self {
    Orchestrator { data }
  }

  pub fn add_component(&mut self, component: Component) -> &mut Self {
    self.data.add_component(component);
    self
  }

  pub fn add_root_component(&mut self, component: Component) -> &mut Self {
    self.data.add_root_component(component);
    self
  }

  pub fn run(&mut self) -> &mut Self {
    while Orchestrator::step(
      &mut self.data.active_instance_ixs,
      &mut self.data.clock_cycle,
      self.data.instance_graph.clone(),
      self.data.instance_active_stream.clone(),
      self.data.signal_connector_stream.clone(),
    ) {}

    self
  }

  fn get_instance<'b>(
    instance_ref_node: &'b mut InstanceRefNode,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    instance_active_stream: Rc<RefCell<Stream<(NodeIndex, bool)>>>,
    components: &HashMap<String, Component>,
  ) -> Rc<RefCell<ComponentInstance>> {
    let (instance_ix, instance) =
      get_or_create_instance_graph_node(instance_ref_node, instance_graph.clone());

    // Get or create ComponentInstance
    match instance {
      Some(instance) => instance.clone(),
      None => {
        // We need to create instance and update InstanceGraph with corresponding nodes and connections
        let component = components
          .get(&mut instance_ref_node.component_name)
          .expect("component not found");
        let instance = Rc::new(RefCell::new(ComponentInstance::new(
          instance_ref_node.node_name.to_string(),
          component,
          &[],
        )));
        let instance_node = InstanceGraphNode {
          component_name: instance_ref_node.node_name.to_string(),
          instance: Some(instance),
        };
        let instance_ix = instance_graph.borrow_mut().add_node(instance_node);
        instance_ref_node.instance_ix = Some(instance_ix);
        let instance = instance_graph.borrow()[instance_ix]
          .instance
          .as_ref()
          .unwrap()
          .clone();

        {
          // Create uninstantiated InstanceGraphNodes for each of the instance's InstanceRefNode.
          // Update the InstanceRefNodes with the index of the InstanceGraphNodes.
          // Add InstanceConnection edges between the InstanceGraphNodes based on field
          // instance_connector_name of instance's Connection edges.

          // Satisfy borrow checker with a separate Vec<NodeIndex>
          let component_ref_node_ixs: Vec<_> = instance
            .borrow()
            .component
            .graph
            .node_indices()
            .filter(|ix| match component.graph[*ix] {
              Node::Component(_) => true,
              _ => false,
            })
            .collect();

          for component_source_ix in component_ref_node_ixs {
            let mut component_edges = component
              .graph
              .neighbors_directed(component_source_ix, Direction::Outgoing)
              .detach();

            while let Some((component_edge_ix, component_target_ix)) =
              component_edges.next(&component.graph)
            {
              match instance.borrow().component.graph[component_edge_ix] {
                Edge::Connection(ref child_connection) => {
                  match (
                    &mut instance.borrow_mut().component.graph[component_source_ix],
                    &mut instance.borrow_mut().component.graph[component_target_ix],
                  ) {
                    (
                      Node::ConnectorOut(child_connector_out),
                      Node::Component(child_instance_ref_node_to),
                    ) => {
                      // From child ConnectorOut to new InstanceRefNode
                      let (child_instance_graph_node_ix_to, child_instance_graph_node_to) =
                        get_or_create_instance_graph_node(
                          child_instance_ref_node_to,
                          instance_graph.clone(),
                        );

                      let child_instance_graph_node_to =
                        &instance_graph.borrow()[child_instance_graph_node_ix_to];

                      // Get NodeIndex of named connector in child instance
                      let child_instance_connector_ix_to = get_connector_index_by_name(
                        components,
                        child_instance_graph_node_to.component_name.as_str(),
                        child_connection.instance_connector_name.clone(),
                      );

                      child_connector_out.to_instance_connector = Some(InstanceConnectorRef {
                        instance_ix: child_instance_graph_node_ix_to,
                        connector_ix: child_instance_connector_ix_to,
                      });
                      child_instance_ref_node_to.instance_ix =
                        Some(child_instance_graph_node_ix_to);
                      instance_graph.borrow_mut().update_edge(
                        child_instance_graph_node_ix_to,
                        instance_ix,
                        InstanceConnection {
                          from_connector_index: component_source_ix,
                          to_connector_index: component_target_ix,
                        },
                      );
                    }
                    (Node::Component(child_instance_ref_node_from), Node::ConnectorIn(_)) => {
                      // From new InstanceRefNode to child ConnectorIn
                      let (child_instance_graph_node_ix, _) = get_or_create_instance_graph_node(
                        child_instance_ref_node_from,
                        instance_graph.clone(),
                      );
                      child_instance_ref_node_from.instance_ix = Some(child_instance_graph_node_ix);
                      instance_graph.borrow_mut().update_edge(
                        child_instance_graph_node_ix,
                        instance_ix,
                        InstanceConnection {
                          from_connector_index: component_source_ix,
                          to_connector_index: component_target_ix,
                        },
                      );
                    }
                    _ => {}
                  }
                }
                _ => {
                  continue;
                }
              }
            }
          }
        }

        // Update stream to include events from new instance
        instance_active_stream.replace_with(|old| {
          let stream = instance.borrow().active_stream();
          let instance_graph_index = instance_ref_node.instance_ix.unwrap();
          old.merge(&stream.map(move |active| (instance_graph_index, active)))
        });
        instance
      }
    }
  }

  fn step(
    active_instance_ixs: &mut HashSet<NodeIndex>,
    clock_cycle: &mut usize,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    instance_active_stream: Rc<RefCell<Stream<(NodeIndex, bool)>>>,
    signal_connector_stream: Rc<RefCell<Stream<InstanceConnectorRef>>>,
  ) -> bool {
    *clock_cycle += 1;

    let mut instance_graph = instance_graph.borrow_mut();
    for ix in active_instance_ixs.iter() {
      let instance = instance_graph[*ix].instance.as_mut().unwrap();
      instance.borrow_mut().step();
    }

    for InstanceConnectorRef {
      instance_ix,
      connector_ix,
    } in signal_connector_stream.borrow().events()
    {
      todo!();
      //Orchestrator::signal_instance_connector(&mut self, instance_ix, connector_ix)
    }

    for (instance_ix, is_active) in instance_active_stream.borrow().events() {
      match is_active {
        true => active_instance_ixs.insert(instance_ix),
        false => active_instance_ixs.remove(&instance_ix),
      };
    }

    active_instance_ixs.len() > 0
  }

  /// Sends a signal to given node of root instance
  pub fn signal_root_instance_connector(&mut self, connector_index: NodeIndex) -> &mut Self {
    //todo: make an enum for passing in NodeIndex or NodeName(string)

    let root_instance_ref = self
      .data
      .root_instance_ref
      .as_ref()
      .expect("No root instance")
      .clone();

    self.signal_instance_connector(&mut root_instance_ref.borrow_mut(), connector_index);

    self
  }

  pub fn signal_instance_connector(
    &mut self,
    instance_ref: &mut InstanceRefNode,
    connector_index: NodeIndex,
  ) {
    let instance = Orchestrator::get_instance(
      instance_ref,
      self.data.instance_graph.clone(),
      self.data.instance_active_stream.clone(),
      &self.data.components,
    );
    instance.borrow_mut().signal_connector_in(connector_index);
  }
}

fn get_connector_index_by_name(
  components: &HashMap<String, Component>,
  component_name: &str,
  connector_name: Rc<str>,
) -> NodeIndex {
  let component = &components[component_name];
  let connector_ix = component
    .graph
    .node_indices()
    .find(|ix| match &component.graph[*ix] {
      Node::ConnectorIn(connector_in) => connector_in.node_name.as_str() == connector_name.as_ref(),
      _ => false,
    })
    .expect("ConnectorIn not found");
  connector_ix
}

fn get_or_create_instance_graph_node(
  instance_ref_node: &mut InstanceRefNode,
  instance_graph: Rc<RefCell<InstanceGraph>>,
) -> (NodeIndex, Option<Rc<RefCell<ComponentInstance>>>) {
  match instance_ref_node.instance_ix {
    Some(instance_ix) => (
      instance_ix,
      instance_graph.borrow()[instance_ix].instance.clone(),
    ),
    None => {
      let instance_ix = instance_graph.borrow_mut().add_node(InstanceGraphNode {
        component_name: instance_ref_node.node_name.to_string(),
        instance: None,
      });
      instance_ref_node.instance_ix = Some(instance_ix);
      (instance_ix, None)
    }
  }
}

#[derive(Debug, Clone)]
pub enum SignalConnectorOptions {
  ConnectorInIndex(NodeIndex),
  ConnectorInIndexForInstanceId(NodeIndex, Rc<str>),
  ConnectorOutIndexForInstanceId(NodeIndex, Rc<str>),
}

#[cfg(test)]
mod tests {
  use super::*;
  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works<'a>() {
    let mut component = Component::new("AComponent".to_string());

    let connector_in = component
      .graph
      .add_node(Node::ConnectorIn(ConnectorInNode::new(
        "connector_in".to_string(),
      )));
    let cell_b = component.graph.add_node(Node::Cell(CellNode::relay()));
    let cell_c = component.graph.add_node(Node::Cell(CellNode::relay()));
    let cell_d = component.graph.add_node(Node::Cell(CellNode::relay()));
    component
      .graph
      .add_edge(connector_in, cell_b, Edge::new_signal(0));
    component.graph.add_edge(cell_b, cell_c, Edge::Association);
    component
      .graph
      .add_edge(cell_b, cell_d, Edge::new_signal(0));

    let mut data = OrchestratorData::new();
    let mut orchestrator = Orchestrator::new(&mut data);
    orchestrator
      .add_root_component(component)
      .signal_root_instance_connector(connector_in)
      .run();

    assert_eq!(orchestrator.data.clock_cycle, 3);
  }

  #[traced_test]
  #[test]
  fn it_works2() {
    // Component1 is instantiated by and connected from Component2
    let mut component_1 = Component::new("Component1".to_string());
    let connector_in_component_1 =
      component_1
        .graph
        .add_node(Node::ConnectorIn(ConnectorInNode::new(
          "connector_in".to_string(),
        )));
    let cell_a_component_1 = component_1.graph.add_node(Node::Cell(CellNode::relay()));
    component_1.graph.add_edge(
      connector_in_component_1,
      cell_a_component_1,
      Edge::new_signal(0),
    );

    let mut component_2 = Component::new("Component2".to_string());
    let connector_in_component_2 =
      component_2
        .graph
        .add_node(Node::ConnectorIn(ConnectorInNode::new(
          "connector_in".to_string(),
        )));
    let cell_a_component_2 = component_2.graph.add_node(Node::Cell(CellNode::relay()));
    let connector_out_component_2 = component_2
      .graph
      .add_node(Node::ConnectorOut(ConnectorOutNode::new()));
    let instance_component_1 = component_2
      .graph
      .add_node(Node::Component(InstanceRefNode::new(
        "component_1".to_string(),
        "Component1".to_string(),
      )));

    component_2.graph.add_edge(
      connector_in_component_2,
      cell_a_component_2,
      Edge::new_signal(0),
    );
    component_2.graph.add_edge(
      cell_a_component_2,
      connector_out_component_2,
      Edge::new_signal(0),
    );
    component_2.graph.add_edge(
      connector_out_component_2,
      instance_component_1,
      Edge::Connection(Connection::new("connector_in".to_string())),
    );

    let mut data = OrchestratorData::new();
    let mut orchestrator = Orchestrator::new(&mut data);
    orchestrator
      .add_root_component(component_2)
      .add_component(component_1)
      .signal_root_instance_connector(connector_in_component_2)
      .run();

    assert_eq!(orchestrator.data.clock_cycle, 3);
  }
}
