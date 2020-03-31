use std::io;
use jack::{ClosureProcessHandler, ProcessHandler, Control, Client, ProcessScope};
use std::borrow::Borrow;


//struct Passthrough;
//
//impl jack::ProcessHandler for Passthrough {
//
////type SomeFn<'a, 'b> = ClosureProcessHandler<fn(&'a jack::Client, &'b jack::ProcessScope) -> jack::Control>;
//
//    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
////            let out_a_p = out_a.as_mut_slice(ps);
////            let out_b_p = out_b.as_mut_slice(ps);
////            let in_a_p = in_a.as_slice(ps);
////            let in_b_p = in_b.as_slice(ps);
////            out_a_p.clone_from_slice(&in_a_p);
////            out_b_p.clone_from_slice(&in_b_p);
//        jack::Control::Continue
//    }
//}

//pub struct Holder<'a> {
//    pub cb: &'static mut (dyn Send + Sync + FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control),
//    pub cb2: &'static mut (dyn Send + Sync + FnMut()),
//    pub cb2: Box<dyn Send + Sync + 'static + FnMut()>,
//    pub cb: Box<dyn 'static + Send + Sync + FnMut(&jack::Client, &jack::ProcessScope, Vec<Vec<jack::Port<jack::AudioIn>>>, Vec<Vec<jack::Port<jack::AudioOut>>>) -> jack::Control>,

//    in_ports: &'a Vec<jack::Port<jack::AudioIn>>,
//    out_ports: &'a Vec<jack::Port<jack::AudioOut>>,
//}
pub struct Holder {}

impl jack::ProcessHandler for Holder {


//type SomeFn<'a, 'b> = ClosureProcessHandler<fn(&'a jack::Client, &'b jack::ProcessScope) -> jack::Control>;

    fn process(&mut self, client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
//        let mi : &mut (dyn std::ops::FnMut() + std::marker::Send + std::marker::Sync) = self.cb2;
//        let mu = *self.cb2;
//        return (self.cb)(client, ps, self.in_ports, self.out_ports);

//
//        mu();
//        (self.cb2)();
//        (self.cb2)();
//        let cb = self.cb;
//        cb(client, ps);
//        (self..cb)(client, ps);

//            let out_a_p = out_a.as_mut_slice(ps);
//            let out_b_p = out_b.as_mut_slice(ps);
//            let in_a_p = in_a.as_slice(ps);
//            let in_b_p = in_b.as_slice(ps);
//            out_a_p.clone_from_slice(&in_a_p);
//            out_b_p.clone_from_slice(&in_b_p);
        jack::Control::Continue
    }
}


pub(crate) struct JackClient
{
    client: Option<jack::Client>,
    //    active_client: Option<jack::AsyncClient<Notifications, Passthrough>>,
//    active_client: Option<jack::AsyncClient<Notifications, Holder<'a>>>,
//  j active_client: Option<jack::AsyncClient<Notifications,
//        jack::ClosureProcessHandler<&'static (dyn Send + Sync + FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control)>
//    >>,

    active_client:
    Option<
        jack::AsyncClient<
            Notifications,
            Holder
        >
    >,
//    foo: Box<jack::AsyncClient<jack::NotificationHandler, jack::ProcessHandler>>,
//    mi: Box<jack::AsyncClient<Notifications, jack::ProcessHandler>>,

    in_ports: Vec<&'static jack::Port<jack::AudioIn>>,
//    out_ports: Vec<jack::Port<jack::AudioOut>>,

//foo: 'static + Send + Sync + FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control,

//    foo: &'static (dyn Send + Sync + FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control),
//    handler: jack::ProcessHandler,

//    foo: jack::ClosureProcessHandler,
}


impl JackClient
{
    pub fn new(name: &str) -> Result<Self, bool> {
        let (client, _status) = jack::Client::new(name, jack::ClientOptions::NO_START_SERVER).unwrap();


//        let in_a = client
//            .register_port("rust_in_l", jack::AudioIn::default())
//            .unwrap();
//        let in_b = &client
//            .register_port("rust_in_r", jack::AudioIn::default())
//            .unwrap();
//        let mut out_a = client
//            .register_port("rust_out_l", jack::AudioOut::default())
//            .unwrap();
//        let mut out_b = &client
//            .register_port("rust_out_r", jack::AudioOut::default())
//            .unwrap();

        return Ok(JackClient {
            client: Some(client),
            active_client: None,
//            in_ports: vec![],
//            out_ports: vec![],
            in_ports: vec![],
        });
    }

    pub fn start<>(
        &mut self,
//                 process_func: Box<dyn 'a+ Send + Sync + FnMut(&jack::Client, &jack::ProcessScope, Vec<Vec<jack::Port<jack::AudioIn>>>, Vec<Vec<jack::Port<jack::AudioOut>>>) -> jack::Control>,
//                 process_func: Box<dyn  Send + Sync + FnMut(&jack::Client, &jack::ProcessScope, Vec<Vec<jack::Port<jack::AudioIn>>>, Vec<Vec<jack::Port<jack::AudioOut>>>) -> jack::Control>,
//        process_func: FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control,
    )
    {
        let client = std::mem::replace(&mut self.client, None).unwrap();

        let in_a = client
            .register_port("rust_in_l", jack::AudioIn::default())
            .unwrap();
        let in_b = client
            .register_port("rust_in_r", jack::AudioIn::default())
            .unwrap();
        let mut out_a = client
            .register_port("rust_out_l", jack::AudioOut::default())
            .unwrap();
        let mut out_b = client
            .register_port("rust_out_r", jack::AudioOut::default())
            .unwrap();


//    let process_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
////        let mut in_a_ref = &in_a;
////        self.in_ports.append(in_a_ref);
//
//        let out_a_p = out_a.as_mut_slice(ps);
//        let out_b_p = out_b.as_mut_slice(ps);
//        let in_a_p = in_a.as_slice(ps);
//        let in_b_p = in_b.as_slice(ps);
//        out_a_p.clone_from_slice(&in_a_p);
//        out_b_p.clone_from_slice(&in_b_p);
//        jack::Control::Continue
//    };
//    let process = jack::ClosureProcessHandler::new(process_callback);

        // Activate the client, which starts the processing.

        let holder = Holder {};
        let active_client = client.activate_async(Notifications, holder).unwrap();

        self.active_client = Some(active_client);

//        let mi: Box<jack::ProcessHandler>;


//        println!("Press enter/return to quit...");
//        let mut user_input = String::new();
//        io::stdin().read_line(&mut user_input).ok();
//
//        active_client.deactivate().unwrap();

//        let client = self.client.unwrap();

//        let process_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
////            let out_a_p = out_a.as_mut_slice(ps);
////            let out_b_p = out_b.as_mut_slice(ps);
////            let in_a_p = in_a.as_slice(ps);
////            let in_b_p = in_b.as_slice(ps);
////            out_a_p.clone_from_slice(&in_a_p);
////            out_b_p.clone_from_slice(&in_b_p);
//            jack::Control::Continue
//        };
//        let process = jack::ClosureProcessHandler::new(process_callback);
//        let jack_process = jack::ClosureProcessHandler::new(Passthrough.process());

//        let p = jack::ProcessHandler::new(process);
//        let jack_process = &jack::ClosureProcessHandler::new(process_func);

//        let jack_process = jack::ClosureProcessHandler::new(*process_func);
//
//        let notification = Notifications;
//
//        let processor = Holder {
//           cb : process_func,
//            in_ports: &self.in_ports,
//            out_ports: &self.out_ports,
//        };

//        self.hold = Some(processor);

//        let ow = client.activate_async(notification, jack_process).unwrap();
//        let ow = client.activate_async(notification, processor).unwrap();
//        println!("{:?}",ow);
//        self.active_client = Some(Box::new(active_client));
//        self.active_client = Some(active_client);
    }

    pub fn stop(self) {
//        self.active_client.as_ref().unwrap().deactivate().unwrap();
//        self.active_client.unwrap().deactivate().unwrap();
    }
}


struct Notifications;

impl jack::NotificationHandler for Notifications {
//    fn thread_init(&self, _: &jack::Client) {
//        println!("JACK: thread init");
//    }
//
//    fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
//        println!(
//            "JACK: shutdown with status {:?} because \"{}\"",
//            status, reason
//        );
//    }
//
//    fn buffer_size(&mut self, _: &jack::Client, sz: jack::Frames) -> jack::Control {
//        println!("JACK: buffer size changed to {}", sz);
//        jack::Control::Continue
//    }
//
//    fn sample_rate(&mut self, _: &jack::Client, srate: jack::Frames) -> jack::Control {
//        println!("JACK: sample rate changed to {}", srate);
//        jack::Control::Continue
//    }
//
//    fn client_registration(&mut self, _: &jack::Client, name: &str, is_reg: bool) {
//        println!(
//            "JACK: {} client with name \"{}\"",
//            if is_reg { "registered" } else { "unregistered" },
//            name
//        );
//    }
//
//    fn port_registration(&mut self, _: &jack::Client, port_id: jack::PortId, is_reg: bool) {
//        println!(
//            "JACK: {} port with id {}",
//            if is_reg { "registered" } else { "unregistered" },
//            port_id
//        );
//    }

//    fn port_rename(
//        &mut self,
//        _: &jack::Client,
//        port_id: jack::PortId,
//        old_name: &str,
//        new_name: &str,
//    ) -> jack::Control {
//        println!(
//            "JACK: port with id {} renamed from {} to {}",
//            port_id, old_name, new_name
//        );
//        jack::Control::Continue
//    }

//    fn ports_connected(
//        &mut self,
//        _: &jack::Client,
//        port_id_a: jack::PortId,
//        port_id_b: jack::PortId,
//        are_connected: bool,
//    ) {
//        println!(
//            "JACK: ports with id {} and {} are {}",
//            port_id_a,
//            port_id_b,
//            if are_connected {
//                "connected"
//            } else {
//                "disconnected"
//            }
//        );
//    }
//
//    fn graph_reorder(&mut self, _: &jack::Client) -> jack::Control {
//        println!("JACK: graph reordered");
//        jack::Control::Continue
//    }
//
//    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
//        println!("JACK: xrun occurred");
//        jack::Control::Continue
//    }
//
//    fn latency(&mut self, _: &jack::Client, mode: jack::LatencyType) {
//        println!(
//            "JACK: {} latency has changed",
//            match mode {
//                jack::LatencyType::Capture => "capture",
//                jack::LatencyType::Playback => "playback",
//            }
//        );
//    }
}
