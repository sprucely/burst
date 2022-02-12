use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableGraph;
use petgraph::Direction;

use crate::component::*;
use crate::component_instance::*;
use std::cell::RefCell;
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub(crate) struct ExecutionContext {
  active_instance_ixs: Vec<NodeIndex>,
  queued_instance_ixs: Vec<NodeIndex>,
  signaled_connector_ixs: Vec<InstanceConnectorIx>,
}

impl ExecutionContext {
  pub fn new() -> Self {
    ExecutionContext {
      active_instance_ixs: Vec::new(),
      queued_instance_ixs: Vec::new(),
      signaled_connector_ixs: Vec::new(),
    }
  }

  pub fn queue_active_instance(&mut self, instance_ix: NodeIndex) {
    self.queued_instance_ixs.push(instance_ix);
  }

  fn start_cycle(&mut self) {
    if self.active_instance_ixs.len() == 0 {
      std::mem::swap(&mut self.active_instance_ixs, &mut self.queued_instance_ixs);
    }
  }

  fn end_cycle(&mut self) -> bool {
    self.active_instance_ixs.clear();
    self.queued_instance_ixs.len() > 0
  }

  pub(crate) fn signal_connector(&mut self, instance_con_ix: InstanceConnectorIx) {
    self.signaled_connector_ixs.push(instance_con_ix);
  }
}

#[derive(Debug)]
pub struct OrchestratorData {
  components: HashMap<String, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  clock_cycle: usize,
  // keep track of all connections between component instances
  pub(crate) instance_graph: Rc<RefCell<InstanceGraph>>,
  root_instance_ref: Option<Rc<RefCell<InstanceRefNode>>>,
  context: ExecutionContext,
}

impl OrchestratorData {
  pub fn new() -> OrchestratorData {
    OrchestratorData {
      components: HashMap::new(),
      clock_cycle: 0,
      instance_graph: Rc::new(RefCell::new(StableGraph::new())),
      root_instance_ref: None,
      context: ExecutionContext::new(),
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

enum InstanceRef<'a> {
  InstanceRefNode(&'a mut InstanceRefNode),
  InstanceConnectorIx(InstanceConnectorIx),
}

pub enum InstanceConnectorRef<'a> {
  InstanceRefNode(&'a mut InstanceRefNode, NodeIndex),
  InstanceConnectorIx(InstanceConnectorIx),
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
    while Self::step(
      &mut self.data.context,
      &mut self.data.clock_cycle,
      self.data.instance_graph.clone(),
      &self.data.components,
    ) {}

    self
  }

  fn get_instance<'b>(
    instance_ref: &'b mut InstanceRef,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    components: &HashMap<String, Component>,
  ) -> Rc<RefCell<ComponentInstance>> {
    let (instance_ix, instance, instance_ref_node) =
      get_or_create_instance_graph_node(instance_ref, instance_graph.clone());

    // Get or create ComponentInstance
    match instance {
      Some(instance) => instance.clone(),
      None => {
        // We need to create instance and update InstanceGraph with corresponding nodes and connections
        let component_name = instance_graph.borrow()[instance_ix].component_name.clone();

        let component = components
          .get::<str>(component_name.as_ref())
          .expect("component not found");
        let instance = Rc::new(RefCell::new(ComponentInstance::new(
          component_name.clone(),
          component,
          &[],
        )));

        if let Some(instance_ref_node) = instance_ref_node {
          // Put new instance into instance_ref_node
          let instance_node = InstanceGraphNode {
            component_name: component_name,
            instance: Some(instance.clone()),
          };
          let instance_ix = instance_graph.borrow_mut().add_node(instance_node);
          instance_ref_node.instance_ix = Some(instance_ix);
        }

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
                      let mut instance_ref =
                        InstanceRef::InstanceRefNode(child_instance_ref_node_to);
                      let (child_instance_graph_node_ix_to, _, _) =
                        get_or_create_instance_graph_node(
                          &mut instance_ref,
                          instance_graph.clone(),
                        );

                      let child_instance_connector_ix_to: NodeIndex;
                      {
                        let instance_graph = instance_graph.borrow();
                        let child_component_name = instance_graph[child_instance_graph_node_ix_to]
                          .component_name
                          .as_str();

                        // Get NodeIndex of named connector in child instance
                        child_instance_connector_ix_to = get_connector_index_by_name(
                          components,
                          child_component_name,
                          child_connection.instance_connector_name.clone(),
                        );

                        child_connector_out.to_instance_connector = Some(InstanceConnectorIx {
                          instance_ix: child_instance_graph_node_ix_to,
                          connector_ix: child_instance_connector_ix_to,
                        });
                      }
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
                      let (child_instance_graph_node_ix, _, _) = get_or_create_instance_graph_node(
                        &mut InstanceRef::InstanceRefNode(child_instance_ref_node_from),
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

        instance
      }
    }
  }

  fn step(
    context: &mut ExecutionContext,
    clock_cycle: &mut usize,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    components: &HashMap<String, Component>,
  ) -> bool {
    *clock_cycle += 1;
    context.start_cycle();

    {
      let mut instance_graph = instance_graph.borrow_mut();
      for ix in context.active_instance_ixs.clone().iter() {
        let instance = instance_graph[*ix].instance.as_mut().unwrap();
        if instance.borrow_mut().step(context) {
          context.queued_instance_ixs.push(*ix);
        }
      }
    }

    for instance_connector_ix in context.signaled_connector_ixs.iter() {
      let instance = Self::get_instance(
        &mut InstanceRef::InstanceConnectorIx(*instance_connector_ix),
        instance_graph.clone(),
        components,
      );

      instance
        .borrow_mut()
        .signal_connector_in(instance_connector_ix.connector_ix);

      context
        .queued_instance_ixs
        .push(instance_connector_ix.instance_ix);
    }

    context.end_cycle()
  }

  /// Sends a signal to given node of root instance
  pub fn signal_root_instance_connector_in(&mut self, connector_index: NodeIndex) -> &mut Self {
    //todo: make an enum for passing in NodeIndex or NodeName(string)

    let root_instance_ref = self
      .data
      .root_instance_ref
      .as_ref()
      .expect("No root instance")
      .clone();

    Self::signal_instance_connector_in(
      &mut InstanceConnectorRef::InstanceRefNode(
        &mut root_instance_ref.borrow_mut(),
        connector_index,
      ),
      self.data.instance_graph.clone(),
      &mut self.data.context.queued_instance_ixs,
      &self.data.components,
    );

    self
  }

  pub fn signal_instance_connector_in(
    instance_ref: &mut InstanceConnectorRef,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    queued_instance_ixs: &mut Vec<NodeIndex>,
    components: &HashMap<String, Component>,
  ) {
    match instance_ref {
      InstanceConnectorRef::InstanceRefNode(instance_ref_node, connector_index) => {
        let instance = Self::get_instance(
          &mut InstanceRef::InstanceRefNode(instance_ref_node),
          instance_graph.clone(),
          components,
        );
        instance.borrow_mut().signal_connector_in(*connector_index);
        queued_instance_ixs.push(instance_ref_node.instance_ix.expect("no instance_ix"));
      }
      InstanceConnectorRef::InstanceConnectorIx(instance_connector_ix) => {
        let instance = Self::get_instance(
          &mut InstanceRef::InstanceConnectorIx(*instance_connector_ix),
          instance_graph.clone(),
          components,
        );
        instance
          .borrow_mut()
          .signal_connector_in(instance_connector_ix.connector_ix);
        queued_instance_ixs.push(instance_connector_ix.instance_ix);
      }
    }
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

fn get_or_create_instance_graph_node<'a>(
  instance_ref: &'a mut InstanceRef,
  instance_graph: Rc<RefCell<InstanceGraph>>,
) -> (
  NodeIndex,
  Option<Rc<RefCell<ComponentInstance>>>,
  Option<&'a mut InstanceRefNode>,
) {
  match instance_ref {
    InstanceRef::InstanceRefNode(ref mut instance_ref_node) => {
      match instance_ref_node.instance_ix {
        Some(instance_ix) => {
          let instance = instance_graph.borrow()[instance_ix].instance.clone();
          //component_name = Ref::map(instance_graph, |g| g[instance_ix].component_name.as_str());
          (instance_ix, instance, Some(instance_ref_node))
        }
        None => {
          let instance_ix = instance_graph.borrow_mut().add_node(InstanceGraphNode {
            component_name: instance_ref_node.component_name.to_string(),
            instance: None,
          });
          instance_ref_node.instance_ix = Some(instance_ix);
          // let component_name = Ref::map(instance_graph.borrow(), |g| {
          //   g[instance_ix].component_name.as_str()
          // });
          (instance_ix, None, Some(instance_ref_node))
        }
      }
    }
    InstanceRef::InstanceConnectorIx(ref instance_connector_ix) => {
      let instance_graph = instance_graph.borrow();
      let instance = instance_graph[instance_connector_ix.instance_ix]
        .instance
        .clone();
      // let component_name = Ref::map(instance_graph, |g| {
      //   g[instance_connector_ix.instance_ix].component_name.as_str()
      // });
      (instance_connector_ix.instance_ix, instance, None)
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
      .signal_root_instance_connector_in(connector_in)
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
      .signal_root_instance_connector_in(connector_in_component_2)
      .run();

    assert_eq!(orchestrator.data.clock_cycle, 3);
  }
}
