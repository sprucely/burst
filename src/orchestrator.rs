use petgraph::graph::EdgeIndex;
use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableGraph;

use crate::component::*;
use crate::instance::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::IndexMut;
use std::rc::Rc;

// TODO: Add threadpool concurrency via rayon crate (https://docs.rs/rayon/)
// exellent summary of various crates at https://www.reddit.com/r/rust/comments/djzd5t/which_asyncconcurrency_crate_to_choose_from/

// TODO: Add error handling via anyhow crate (https://docs.rs/anyhow/)
// summary of error handling at https://www.reddit.com/r/rust/comments/gqe57x/what_are_you_using_for_error_handling/
// anyhow for applications, thiserror for libraries (thiserror helps to not expose internal error handling to users)

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
  signaled_connector_ixs: Vec<InstanceComponentIx>,
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
    self.signaled_connector_ixs.clear();
    self.queued_instance_ixs.len() > 0
  }

  pub(crate) fn signal_connector(&mut self, instance_con_ix: InstanceComponentIx) {
    self.signaled_connector_ixs.push(instance_con_ix);
    self.queued_instance_ixs.push(instance_con_ix.instance_ix);
  }
}

enum InstanceRef<'a> {
  InstanceRefNode(&'a mut InstanceRefNode),
  InstanceConnectorIx(InstanceComponentIx),
}

pub enum InstanceConnectorRef<'a> {
  InstanceRefNode(&'a mut InstanceRefNode, NodeIndex),
  InstanceConnectorIx(InstanceComponentIx),
}

#[derive(Debug)]
pub struct Orchestrator {
  components: HashMap<Rc<str>, Component>,
  // TODO: (microoptimization) Sort instances topologically for cache locality purposes
  clock_cycle: usize,
  // keep track of all connections between component instances
  pub(crate) instance_graph: Rc<RefCell<InstanceGraph>>,
  root_instance_ref: Option<Rc<RefCell<InstanceRefNode>>>,
  context: ExecutionContext,
}

impl Orchestrator {
  pub fn new() -> Self {
    Orchestrator {
      components: HashMap::new(),
      clock_cycle: 0,
      instance_graph: Rc::new(RefCell::new(StableGraph::new())),
      root_instance_ref: None,
      context: ExecutionContext::new(),
    }
  }

  pub fn add_component(&mut self, component: Component) -> &mut Self {
    self.components.insert(component.name.clone(), component);
    self
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

  pub fn run(&mut self) -> &mut Self {
    while Self::step(
      &mut self.context,
      &mut self.clock_cycle,
      self.instance_graph.clone(),
      &self.components,
    ) {}

    self
  }

  fn get_instance<'b>(
    instance_ref: &'b mut InstanceRef,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    components: &HashMap<Rc<str>, Component>,
  ) -> Rc<RefCell<Instance>> {
    let (instance_ix, instance, instance_ref_node) =
      get_or_create_instance_graph_node(instance_ref, instance_graph.clone());

    // Get or create Instance
    match instance {
      Some(instance) => instance.clone(),
      None => {
        // We need to create instance and update InstanceGraph with corresponding nodes and connections
        let component_name = instance_graph.borrow()[instance_ix].component_name.clone();

        let component = components
          .get::<str>(component_name.as_ref())
          .expect("component not found");
        let instance = Rc::new(RefCell::new(Instance::new(
          component_name.clone(),
          component,
          &[],
        )));

        if let Some(instance_ref_node) = instance_ref_node {
          // Put new instance into instance_ref_node
          instance_ref_node.instance_ix = Some(instance_ix);
        }

        instance_graph.borrow_mut()[instance_ix].instance = Some(instance.clone());

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

          for component_ref_node_ix in component_ref_node_ixs {
            let mut component_edges = component
              .graph
              .neighbors_undirected(component_ref_node_ix)
              .detach();

            while let Some((component_edge_ix, component_target_ix)) =
              component_edges.next(&component.graph)
            {
              let connected_nodes = unsafe {
                // It's safe to assume these three mutable references don't alias
                let graph = &mut instance.borrow_mut().component.graph as *mut _;
                (
                  <ComponentGraph as IndexMut<NodeIndex>>::index_mut(
                    &mut *graph,
                    component_ref_node_ix,
                  ),
                  <ComponentGraph as IndexMut<EdgeIndex>>::index_mut(
                    &mut *graph,
                    component_edge_ix,
                  ),
                  <ComponentGraph as IndexMut<NodeIndex>>::index_mut(
                    &mut *graph,
                    component_target_ix,
                  ),
                )
              };
              match connected_nodes {
                (
                  Node::Component(ref mut child_instance_ref_node_to),
                  Edge::Connection(ref child_connection),
                  Node::ConnectorOut(ref mut child_connector_out),
                ) => {
                  // From child ConnectorOut to new InstanceRefNode
                  let mut instance_ref = InstanceRef::InstanceRefNode(child_instance_ref_node_to);
                  let (child_instance_graph_node_ix_to, _, _) =
                    get_or_create_instance_graph_node(&mut instance_ref, instance_graph.clone());

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

                    child_connector_out.to_instance_connector = Some(InstanceComponentIx {
                      instance_ix: child_instance_graph_node_ix_to,
                      component_ix: child_instance_connector_ix_to,
                    });
                  }
                  child_instance_ref_node_to.instance_ix = Some(child_instance_graph_node_ix_to);
                  instance_graph.borrow_mut().update_edge(
                    child_instance_graph_node_ix_to,
                    instance_ix,
                    InstanceConnection {
                      from_connector_index: component_ref_node_ix,
                      to_connector_index: component_target_ix,
                    },
                  );
                }
                (
                  Node::Component(ref mut child_instance_ref_node_from),
                  Edge::Connection(_),
                  Node::ConnectorIn(_),
                ) => {
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
                      from_connector_index: component_ref_node_ix,
                      to_connector_index: component_target_ix,
                    },
                  );
                }
                something_else => {
                  panic!("Unexpected node type: {:?}", something_else);
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
    components: &HashMap<Rc<str>, Component>,
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
        .signal_connector_in(instance_connector_ix.component_ix);

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
      .root_instance_ref
      .as_ref()
      .expect("No root instance")
      .clone();

    Self::signal_instance_connector_in(
      &mut InstanceConnectorRef::InstanceRefNode(
        &mut root_instance_ref.borrow_mut(),
        connector_index,
      ),
      self.instance_graph.clone(),
      &mut self.context.queued_instance_ixs,
      &self.components,
    );

    self
  }

  pub fn signal_instance_connector_in(
    instance_ref: &mut InstanceConnectorRef,
    instance_graph: Rc<RefCell<InstanceGraph>>,
    queued_instance_ixs: &mut Vec<NodeIndex>,
    components: &HashMap<Rc<str>, Component>,
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
          .signal_connector_in(instance_connector_ix.component_ix);
        queued_instance_ixs.push(instance_connector_ix.instance_ix);
      }
    }
  }
}

fn get_connector_index_by_name(
  components: &HashMap<Rc<str>, Component>,
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
  Option<Rc<RefCell<Instance>>>,
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
  use petgraph::dot::Dot;
  use tracing::trace;
  use tracing_test::traced_test;

  #[traced_test]
  #[test]
  fn it_works<'a>() {
    let mut component = Component::new("AComponent");

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

    let mut orchestrator = Orchestrator::new();
    orchestrator
      .add_root_component(component)
      .signal_root_instance_connector_in(connector_in)
      .run();

    assert_eq!(orchestrator.clock_cycle, 3);
  }

  #[traced_test]
  #[test]
  fn it_works2() {
    // Component1 is instantiated by and connected from Component2
    let mut component_1 = Component::new("Component1");
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

    let mut component_2 = Component::new("Component2");
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
        component_1.name.clone(),
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

    trace!(
      "{:?}",
      Dot::new(&component_2.graph) //, &[Config::EdgeNoLabel])
    );

    let mut orchestrator = Orchestrator::new();
    orchestrator
      .add_root_component(component_2)
      .add_component(component_1)
      .signal_root_instance_connector_in(connector_in_component_2)
      .run();

    assert_eq!(orchestrator.clock_cycle, 4);
  }
}
