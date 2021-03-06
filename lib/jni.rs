extern crate libc;
extern crate debug;

use std::mem;
use std::fmt;
use std::string;
use std::c_str::CString;

use native::*;


#[deriving(Clone)]
pub struct JavaVMOption {
	pub optionString: string::String,
	pub extraInfo: *const ::libc::c_void
}

impl JavaVMOption {
	pub fn new(option: &str, extra: *const ::libc::c_void) -> JavaVMOption {
		JavaVMOption{
			optionString: option.to_string(),
			extraInfo: extra
		}
	}
}

impl fmt::Show for JavaVMOption {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "JavaVMOption [ optionString: \"{}\", extraInfo: {} ]", self.optionString, self.extraInfo)
	}
}


#[deriving(Show, Clone)]
pub struct JavaVMInitArgs {
	pub version: JniVersion,
	pub options: Vec<JavaVMOption>,
	pub ignoreUnrecognized: bool
}

impl JavaVMInitArgs {
	pub fn new(version: JniVersion, options: &[JavaVMOption], ignoreUnrecognized: bool) -> JavaVMInitArgs {
		JavaVMInitArgs{
			version: version,
			options: options.to_owned(),
			ignoreUnrecognized: ignoreUnrecognized
		}
	}
}


#[deriving(Show)]
pub struct JavaVMAttachArgs {
	pub version: JniVersion,
	pub name: string::String,
	pub group: JavaObject
}

impl JavaVMAttachArgs {
	pub fn new(version: JniVersion, name: &str, group: JavaObject) -> JavaVMAttachArgs {
		JavaVMAttachArgs{
			version: version,
			name: name.to_string(),
			group: group
		}
	}
}


#[deriving(Clone)]
pub struct JavaVM {
	ptr: *mut JavaVMImpl,
	version: JniVersion,
	name: CString,
	owned: bool
}

impl JavaVM {
	pub fn new(args: JavaVMInitArgs, name: &str) -> JavaVM {
		let (res, jvm) = unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
			let mut vm_opts = vec![];
			for opt in args.options.iter() {
				vm_opts.push(JavaVMOptionImpl_new(opt));
			}
			let mut argsImpl = JavaVMInitArgsImpl{
				version: args.version,
				nOptions: args.options.len() as jint,
				options: vm_opts.as_mut_ptr(),
				ignoreUnrecognized: args.ignoreUnrecognized as jboolean
			};

			let res = JNI_CreateJavaVM(&mut jvm, &mut env, &mut argsImpl);

			for &i in vm_opts.iter() {
				libc::free(i.optionString as *mut libc::c_void);
			}

			(res, jvm)
		};

		match res {
			JNI_OK => JavaVM{
				ptr: jvm,
				version: args.version,
				name: name.to_c_str(),
				owned: true
			},
			_ => fail!("JNI_CreateJavaVM error: {}", res)
		}
	}

	pub fn from(ptr: *mut JavaVMImpl) -> JavaVM {
		let mut res = JavaVM{
			ptr: ptr,
			version: JNI_VERSION_1_1,
			name: "".to_c_str(),
			owned: false
		};
		res.version = res.get_env().version();
		res
	}

	pub fn ptr(&self) -> *mut JavaVMImpl {
		self.ptr
	}

	pub fn version(&self) -> JniVersion {
		return self.version
	}

	pub fn get_env(&mut self) -> JavaEnv {
		unsafe {
			let mut jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThread)
		}
	}

	pub fn get_env_daemon(&mut self) -> JavaEnv {
		unsafe {
			let mut jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThreadAsDaemon)
		}
	}

	pub fn detach_current_thread(&mut self) -> bool {
		unsafe {
			let mut jni = **self.ptr;
			(jni.DetachCurrentThread)(self.ptr) == JNI_OK
		}
	}

	unsafe fn get_env_gen(&mut self, fun: extern "C" fn(vm: *mut JavaVMImpl, penv: &mut *mut JNIEnvImpl, args: *mut JavaVMAttachArgsImpl) -> JniError) -> JavaEnv {
		let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
		let res = ((**self.ptr).GetEnv)(self.ptr, &mut env, self.version());
		match res {
			JNI_OK => JavaEnv {ptr: env},
			JNI_EDETACHED => {
				let mut attachArgs = JavaVMAttachArgsImpl{
					version: self.version(),
					name: self.name.as_mut_ptr(),
					group: 0 as jobject
				};
				let res = fun(self.ptr, &mut env, &mut attachArgs);
				match res {
					JNI_OK => JavaEnv {ptr: env},
					_ => fail!("AttachCurrentThread error {}!", res)
				}
			},
			JNI_EVERSION => fail!("Version {} is not supported by GetEnv!", self.version()),
			_ => fail!("GetEnv error {}!", res)
		}
	}

	unsafe fn destroy_java_vm(&self) -> bool {
		((**self.ptr).DestroyJavaVM)(self.ptr) == JNI_OK
	}
}


impl fmt::Show for JavaVM {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "JavaVM [ ptr: {} ]", self.ptr)
	}
}

impl Drop for JavaVM {
	fn drop(&mut self) {
		if self.owned {
			unsafe {
				self.destroy_java_vm();
			}
		}
	}
}

#[deriving(Clone)]
pub struct JavaEnv {
	ptr: *mut JNIEnvImpl
}

impl fmt::Show for JavaEnv {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "JavaEnv [ ptr: {} ]", self.ptr)
	}
}

impl JavaEnv {
	pub fn version(&self) -> JniVersion {
		unsafe {
			mem::transmute(((**self.ptr).GetVersion)(self.ptr))
		}
	}

	pub fn ptr(&self) -> *mut JNIEnvImpl {
		self.ptr
	}

	pub fn define_class<T: JObject>(&self, name: &str, loader: &T, buf: &[u8], len: uint) -> JavaClass {
		JObject::from(
			self,
			name.with_c_str(|name| unsafe {
				((**self.ptr).DefineClass)(self.ptr, name, loader.get_obj(), buf.as_ptr() as *const jbyte, len as jsize)
			}) as jobject
		)
	}

	pub fn find_class(&self, name: &str) -> Option<JavaClass> {
		let ptr = name.with_c_str(|name| unsafe {
			((**self.ptr).FindClass)(self.ptr, name)
		});

		if ptr == (0 as jclass) {
			None
		} else {
			Some(JObject::from(self, ptr as jobject))
		}
	}

	pub fn get_super_class(&self, sub: &JavaClass) -> JavaClass {
		JObject::from( self, unsafe {
			((**self.ptr).GetSuperclass)(self.ptr, sub.ptr) as jobject
		})
	}

	pub fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass) -> bool {
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) != 0
		}
	}


	pub fn throw(&self, obj: &JavaThrowable) -> bool {
		unsafe {
			((**self.ptr).Throw)(self.ptr, obj.ptr) == JNI_OK
		}
	}

	pub fn throw_new(&self, clazz: &JavaClass, msg: &str) -> bool {
		msg.with_c_str(|msg| unsafe {
			((**self.ptr).ThrowNew)(self.ptr, clazz.ptr, msg) == JNI_OK
		})
	}

	pub fn exception_occured(&self) -> JavaThrowable {
		JObject::from(
			self,
			unsafe {
				((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
			}
		)
	}

	pub fn exception_describe(&self) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	pub fn exception_clear(&self) {
		unsafe {
			((**self.ptr).ExceptionClear)(self.ptr)
		}
	}

	pub fn fatal_error(&self, msg: &str) {
		msg.with_c_str(|msg| unsafe {
			((**self.ptr).FatalError)(self.ptr, msg)
		})
	}

	pub fn push_local_frame(&self, capacity: int) -> bool {
		unsafe {
			((**self.ptr).PushLocalFrame)(self.ptr, capacity as jint) == JNI_OK
		}
	}

	pub fn pop_local_frame<T: JObject>(&self, result: &T) -> T {
		JObject::from(self, unsafe {
			((**self.ptr).PopLocalFrame)(self.ptr, result.get_obj())
		})
	}

	pub fn is_same_object<T1: JObject, T2: JObject>(&self, obj1: &T1, obj2: &T2) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) != 0
		}
	}

	pub fn is_null<T: JObject>(&self, obj1: &T) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), 0 as jobject) != 0
		}
	}

	fn new_local_ref<T: JObject>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_local_ref<T: JObject>(&self, gobj: &T) {
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_global_ref<T: JObject>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_global_ref<T: JObject>(&self, gobj: &T) {
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_weak_ref<T: JObject>(&self, lobj: &T) -> jweak {
		unsafe {
			((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_weak_ref<T: JObject>(&self, wobj: &T) {
		unsafe {
			((**self.ptr).DeleteWeakGlobalRef)(self.ptr, wobj.get_obj() as jweak)
		}
	}

	pub fn ensure_local_capacity(&self, capacity: int) -> bool {
		unsafe {
			((**self.ptr).EnsureLocalCapacity)(self.ptr, capacity as jint) == JNI_OK
		}
	}

	pub fn alloc_object(&self, clazz: &JavaClass) -> JavaObject {
		JObject::from(self, unsafe {
			((**self.ptr).AllocObject)(self.ptr, clazz.ptr)
		})
	}

	pub fn monitor_enter<T: JObject>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorEnter)(self.ptr, obj.get_obj()) == JNI_OK
		}
	}

	pub fn monitor_exit<T: JObject>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorExit)(self.ptr, obj.get_obj()) == JNI_OK
		}
	}

	pub fn jvm(&self) -> JavaVM {
		JavaVM::from(unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			((**self.ptr).GetJavaVM)(self.ptr, &mut jvm);
			jvm
		})
	}

	pub fn exception_check(&self) -> bool {
		unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) != 0
		}
	}
}

#[deriving(Show)]
enum RefType {
	Local,
	Global,
	Weak
}

pub trait JObject: Drop + Clone {
	fn get_env(&self) -> JavaEnv;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;

	fn from(env: &JavaEnv, ptr: jobject) -> Self;
	fn global(&self) -> Self;
	fn weak(&self) -> Self;

	fn inc_ref(&self) -> jobject {
		let env = self.get_env();
		match self.ref_type() {
			Local => env.new_local_ref(self),
			Global => env.new_global_ref(self),
			Weak => env.new_weak_ref(self) as jobject,
		}
	}

	fn dec_ref(&mut self) {
		let env = self.get_env();
		match self.ref_type() {
			Local => env.delete_local_ref(self),
			Global => env.delete_global_ref(self),
			Weak => env.delete_weak_ref(self)
		}
	}

	fn get_class(&self) -> JavaClass {
		let env = self.get_env();
		JObject::from(&env, unsafe {
			((**env.ptr).GetObjectClass)(env.ptr, self.get_obj()) as jobject
		})
	}

	fn as_jobject(&self) -> JavaObject {
		JavaObject{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.ref_type()
		}
	}

	fn is_instance_of(&self, clazz: &JavaClass) -> bool {
		let env = self.get_env();
		unsafe {
			((**env.ptr).IsInstanceOf)(env.ptr, self.get_obj(), clazz.ptr) != 0
		}
	}

	fn is_same<T: JObject>(&self, val: &T) -> bool {
		self.get_env().is_same_object(self, val)
	}

	fn is_null(&self) -> bool {
		self.get_env().is_null(self)
	}
}

pub trait JArray: JObject {}


macro_rules! impl_jobject_base(
	($cls:ident) => (
		impl Drop for $cls {
			fn drop(&mut self) {
				self.dec_ref();
			}
		}

		impl Clone for $cls {
			fn clone(&self) -> $cls {
				$cls {
					env: self.get_env(),
					ptr: self.inc_ref(),
					rtype: self.rtype
				}
			}
		}
	);
)

macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl_jobject_base!($cls)

		impl JObject for $cls {
			fn get_env(&self) -> JavaEnv {
				self.env
			}

			fn get_obj(&self) -> jobject {
				self.ptr as jobject
			}

			fn ref_type(&self) -> RefType {
				self.rtype
			}

			fn from(env: &JavaEnv, ptr: jobject) -> $cls {
				$cls{
					env: env.clone(),
					ptr: ptr as $native,
					rtype: Local
				}
			}

			fn global(&self) -> $cls {
				let env = self.get_env();
				$cls{
					env: env,
					ptr: env.new_global_ref(self),
					rtype: Global
				}
			}

			fn weak(&self) -> $cls {
				let env = self.get_env();
				$cls{
					env: env,
					ptr: env.new_weak_ref(self),
					rtype: Weak
				}
			}
		}
	);
)

macro_rules! impl_jarray(
	($cls:ident, $native:ident) => (
		impl_jobject!($cls, $native)

		// impl $cls {
		// 	pub fn as_jarray(&self) -> JavaArray {
		// 		self.inc_ref();
		// 		JavaArray {
		// 			env: self.get_env(),
		// 			ptr: self.ptr as jarray
		// 		}
		// 	}
		// }
	);
)



#[deriving(Show)]
pub struct JavaObject {
	env: JavaEnv,
	ptr: jobject,
	rtype: RefType
}

impl_jobject!(JavaObject, jobject)


#[deriving(Show)]
pub struct JavaClass {
	env: JavaEnv,
	ptr: jclass,
	rtype: RefType
}

impl_jobject!(JavaClass, jclass)

impl JavaClass {
	pub fn get_super(&self) -> JavaClass {
		self.get_env().get_super_class(self)
	}

	pub fn alloc(&self) -> JavaObject {
		self.get_env().alloc_object(self)
	}

	pub fn find(env: &JavaEnv, name: &str) -> JavaClass {
		match env.find_class(name) {
			None => fail!("Class \"{}\" not found!", name),
			Some(cls) => cls
		}
	}
}


#[deriving(Show)]
pub struct JavaThrowable {
	env: JavaEnv,
	ptr: jthrowable,
	rtype: RefType
}

impl_jobject!(JavaThrowable, jthrowable)


pub struct JavaString {
	env: JavaEnv,
	ptr: jstring,
	rtype: RefType
}

impl_jobject!(JavaString, jstring)


impl fmt::Show for JavaString {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "JavaString [ env: {}, ptr: {}, chars: {} ]", self.env, self.ptr, self.chars())
	}
}

impl JavaString {
	pub fn new(env: JavaEnv, val: &str) -> JavaString {
		JObject::from(&env, val.with_c_str(|val| unsafe {
			((**env.ptr).NewStringUTF)(env.ptr, val) as jobject
		}))
	}

	pub fn len(&self) -> uint {
		unsafe {
			((**self.get_env().ptr).GetStringLength)(self.get_env().ptr, self.ptr) as uint
		}
	}

	pub fn size(&self) -> uint {
		unsafe {
			((**self.get_env().ptr).GetStringUTFLength)(self.get_env().ptr, self.ptr) as uint
		}
	}

	pub fn to_str(&self) -> string::String {
		self.chars().to_str()
	}

	fn chars(&self) -> JavaStringChars {
		let mut isCopy: jboolean = 0;
		let val = unsafe {
			((**self.get_env().ptr).GetStringUTFChars)(self.get_env().ptr, self.ptr, &mut isCopy)
		};
		JavaStringChars{
			s: self.clone(),
			isCopy: isCopy != 0,
			chars: val
		}
	}

	pub fn region(&self, start: uint, length: uint) -> string::String {
		let size = self.size();
		let mut vec: Vec<u8> = Vec::with_capacity(size);
		unsafe {
			((**self.get_env().ptr).GetStringUTFRegion)(self.get_env().ptr, self.ptr, start as jsize, length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
			vec.set_len(length as uint);
		}

		match string::String::from_utf8(vec) {
			Ok(res) => res,
			Err(_) => "".to_string()
		}
	}
}


struct JavaStringChars {
	s: JavaString,
	isCopy: bool,
	chars: *const ::libc::c_char
}

impl Drop for JavaStringChars {
	fn drop(&mut self) {
		if self.isCopy {
			unsafe {
				((**self.s.env.ptr).ReleaseStringUTFChars)(self.s.env.ptr, self.s.ptr, self.chars)
			}
		}
	}
}

impl fmt::Show for JavaStringChars {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "\"{}\"", self.to_str())
	}
}

impl JavaStringChars {
	fn to_str(&self) -> string::String {
		unsafe {
			let cs = ::std::c_str::CString::new(self.chars, false);
			let s = cs.as_str();
			match s {
				None => "".to_string(),
				Some(s) => s.to_string()
			}
		}
	}
}


// For future
trait JavaPrimitive {}

impl JavaPrimitive for jboolean {}
impl JavaPrimitive for jbyte {}
impl JavaPrimitive for jchar {}
impl JavaPrimitive for jshort {}
impl JavaPrimitive for jint {}
impl JavaPrimitive for jlong {}
impl JavaPrimitive for jfloat {}
impl JavaPrimitive for jdouble {}
// impl JavaPrimitive for jsize {}


pub struct JavaArray<T> {
	env: JavaEnv,
	ptr: jarray,
	rtype: RefType
}

#[unsafe_destructor]
impl<T> Drop for JavaArray<T> {
	fn drop(&mut self) {
		self.dec_ref();
	}
}

impl<T> Clone for JavaArray<T> {
	fn clone(&self) -> JavaArray<T> {
		JavaArray{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.rtype
		}
	}
}

impl<T> JObject for JavaArray<T> {
	fn get_env(&self) -> JavaEnv {
		self.env
	}

	fn get_obj(&self) -> jobject {
		self.ptr as jobject
	}

	fn ref_type(&self) -> RefType {
		self.rtype
	}

	fn from(env: &JavaEnv, ptr: jobject) -> JavaArray<T> {
		JavaArray{
			env: env.clone(),
			ptr: ptr as jarray,
			rtype: Local
		}
	}

	fn global(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env,
			ptr: env.new_global_ref(self),
			rtype: Global
		}
	}

	fn weak(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env,
			ptr: env.new_weak_ref(self),
			rtype: Weak
		}
	}
}


unsafe fn JavaVMOptionImpl_new(opt: &::jni::JavaVMOption) -> JavaVMOptionImpl {
	JavaVMOptionImpl{
		optionString: opt.optionString.to_c_str().unwrap(),
		extraInfo: opt.extraInfo
	}
}
