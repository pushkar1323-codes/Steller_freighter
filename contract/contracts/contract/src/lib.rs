#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    Address, Env, String, Vec, Map,
};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct Post {
    pub id: u64,
    pub author: Address,
    pub title: String,
    pub content: String,
    pub timestamp: u64,
    pub likes: u64,
    pub is_deleted: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct Comment {
    pub id: u64,
    pub post_id: u64,
    pub author: Address,
    pub content: String,
    pub timestamp: u64,
}

// ─── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    PostCount,
    CommentCount,
    Post(u64),
    PostComments(u64),  // Vec<u64> of comment IDs
    Comment(u64),
    AuthorPosts(Address), // Vec<u64> of post IDs by author
    PostLiked(u64, Address), // has address liked post_id?
    Admin,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct DecentralizedBlog;

#[contractimpl]
impl DecentralizedBlog {

    // ── Initialise ────────────────────────────────────────────────────────────

    /// Deploy and set the admin (called once by deployer).
    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PostCount, &0u64);
        env.storage().instance().set(&DataKey::CommentCount, &0u64);
    }

    // ── Posts ─────────────────────────────────────────────────────────────────

    /// Publish a new post. Returns the new post ID.
    pub fn create_post(env: Env, author: Address, title: String, content: String) -> u64 {
        author.require_auth();

        let post_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PostCount)
            .unwrap_or(0);
        let post_id = post_count + 1;

        let post = Post {
            id: post_id,
            author: author.clone(),
            title,
            content,
            timestamp: env.ledger().timestamp(),
            likes: 0,
            is_deleted: false,
        };

        env.storage().persistent().set(&DataKey::Post(post_id), &post);

        // Track posts per author
        let mut author_posts: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::AuthorPosts(author.clone()))
            .unwrap_or(Vec::new(&env));
        author_posts.push_back(post_id);
        env.storage()
            .persistent()
            .set(&DataKey::AuthorPosts(author), &author_posts);

        // Initialise empty comment list for the post
        env.storage()
            .persistent()
            .set(&DataKey::PostComments(post_id), &Vec::<u64>::new(&env));

        env.storage().instance().set(&DataKey::PostCount, &post_id);

        env.events().publish(
            (symbol_short!("post_new"),),
            (post_id,),
        );

        post_id
    }

    /// Update a post's title / content. Only the original author may edit.
    pub fn update_post(
        env: Env,
        author: Address,
        post_id: u64,
        new_title: String,
        new_content: String,
    ) {
        author.require_auth();

        let mut post: Post = env
            .storage()
            .persistent()
            .get(&DataKey::Post(post_id))
            .expect("post not found");

        if post.author != author {
            panic!("only the author can edit this post");
        }
        if post.is_deleted {
            panic!("post has been deleted");
        }

        post.title = new_title;
        post.content = new_content;
        env.storage().persistent().set(&DataKey::Post(post_id), &post);

        env.events().publish(
            (symbol_short!("post_upd"),),
            (post_id,),
        );
    }

    /// Soft-delete a post. Author OR admin may delete.
    pub fn delete_post(env: Env, caller: Address, post_id: u64) {
        caller.require_auth();

        let mut post: Post = env
            .storage()
            .persistent()
            .get(&DataKey::Post(post_id))
            .expect("post not found");

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");

        if post.author != caller && admin != caller {
            panic!("not authorized to delete this post");
        }

        post.is_deleted = true;
        env.storage().persistent().set(&DataKey::Post(post_id), &post);

        env.events().publish(
            (symbol_short!("post_del"),),
            (post_id,),
        );
    }

    /// Like a post. Each address can like a post only once.
    pub fn like_post(env: Env, liker: Address, post_id: u64) {
        liker.require_auth();

        let liked_key = DataKey::PostLiked(post_id, liker.clone());
        if env.storage().persistent().has(&liked_key) {
            panic!("already liked");
        }

        let mut post: Post = env
            .storage()
            .persistent()
            .get(&DataKey::Post(post_id))
            .expect("post not found");

        if post.is_deleted {
            panic!("post has been deleted");
        }

        post.likes += 1;
        env.storage().persistent().set(&DataKey::Post(post_id), &post);
        env.storage().persistent().set(&liked_key, &true);

        env.events().publish(
            (symbol_short!("liked"),),
            (post_id, liker),
        );
    }

    // ── Comments ──────────────────────────────────────────────────────────────

    /// Add a comment to a post. Returns the new comment ID.
    pub fn add_comment(
        env: Env,
        author: Address,
        post_id: u64,
        content: String,
    ) -> u64 {
        author.require_auth();

        // Make sure the post exists and isn't deleted
        let post: Post = env
            .storage()
            .persistent()
            .get(&DataKey::Post(post_id))
            .expect("post not found");
        if post.is_deleted {
            panic!("cannot comment on a deleted post");
        }

        let comment_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CommentCount)
            .unwrap_or(0);
        let comment_id = comment_count + 1;

        let comment = Comment {
            id: comment_id,
            post_id,
            author,
            content,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Comment(comment_id), &comment);

        // Append comment id to post's comment list
        let mut post_comments: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PostComments(post_id))
            .unwrap_or(Vec::new(&env));
        post_comments.push_back(comment_id);
        env.storage()
            .persistent()
            .set(&DataKey::PostComments(post_id), &post_comments);

        env.storage()
            .instance()
            .set(&DataKey::CommentCount, &comment_id);

        env.events().publish(
            (symbol_short!("comment"),),
            (post_id, comment_id),
        );

        comment_id
    }

    // ── Read / Query ──────────────────────────────────────────────────────────

    /// Fetch a single post by ID.
    pub fn get_post(env: Env, post_id: u64) -> Post {
        env.storage()
            .persistent()
            .get(&DataKey::Post(post_id))
            .expect("post not found")
    }

    /// Fetch a single comment by ID.
    pub fn get_comment(env: Env, comment_id: u64) -> Comment {
        env.storage()
            .persistent()
            .get(&DataKey::Comment(comment_id))
            .expect("comment not found")
    }

    /// Get all comment IDs for a given post.
    pub fn get_post_comment_ids(env: Env, post_id: u64) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::PostComments(post_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get all post IDs published by a specific author.
    pub fn get_author_post_ids(env: Env, author: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::AuthorPosts(author))
            .unwrap_or(Vec::new(&env))
    }

    /// Total number of posts ever created (includes deleted).
    pub fn get_post_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PostCount)
            .unwrap_or(0)
    }

    /// Total number of comments ever created.
    pub fn get_comment_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::CommentCount)
            .unwrap_or(0)
    }

    /// Check whether an address has liked a post.
    pub fn has_liked(env: Env, liker: Address, post_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::PostLiked(post_id, liker))
    }

    /// Return the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }

    // ── Admin ─────────────────────────────────────────────────────────────────

    /// Transfer admin rights to a new address.
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if stored_admin != current_admin {
            panic!("caller is not admin");
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, Address, DecentralizedBlogClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, DecentralizedBlog);
        let client = DecentralizedBlogClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_create_and_get_post() {
        let (env, _, client) = setup();
        let author = Address::generate(&env);
        let post_id = client.create_post(
            &author,
            &String::from_str(&env, "Hello Stellar"),
            &String::from_str(&env, "My first on-chain post!"),
        );
        assert_eq!(post_id, 1);
        let post = client.get_post(&post_id);
        assert_eq!(post.id, 1);
        assert_eq!(post.likes, 0);
        assert!(!post.is_deleted);
    }

    #[test]
    fn test_like_post() {
        let (env, _, client) = setup();
        let author = Address::generate(&env);
        let liker = Address::generate(&env);
        let post_id = client.create_post(
            &author,
            &String::from_str(&env, "Likeable post"),
            &String::from_str(&env, "Like me!"),
        );
        client.like_post(&liker, &post_id);
        let post = client.get_post(&post_id);
        assert_eq!(post.likes, 1);
        assert!(client.has_liked(&liker, &post_id));
    }

    #[test]
    fn test_add_comment() {
        let (env, _, client) = setup();
        let author = Address::generate(&env);
        let commenter = Address::generate(&env);
        let post_id = client.create_post(
            &author,
            &String::from_str(&env, "Post with comments"),
            &String::from_str(&env, "Comment on me!"),
        );
        let comment_id = client.add_comment(
            &commenter,
            &post_id,
            &String::from_str(&env, "Great post!"),
        );
        assert_eq!(comment_id, 1);
        let ids = client.get_post_comment_ids(&post_id);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_delete_post() {
        let (env, admin, client) = setup();
        let author = Address::generate(&env);
        let post_id = client.create_post(
            &author,
            &String::from_str(&env, "To be deleted"),
            &String::from_str(&env, "..."),
        );
        client.delete_post(&admin, &post_id);
        let post = client.get_post(&post_id);
        assert!(post.is_deleted);
    }

    #[test]
    fn test_update_post() {
        let (env, _, client) = setup();
        let author = Address::generate(&env);
        let post_id = client.create_post(
            &author,
            &String::from_str(&env, "Old title"),
            &String::from_str(&env, "Old content"),
        );
        client.update_post(
            &author,
            &post_id,
            &String::from_str(&env, "New title"),
            &String::from_str(&env, "New content"),
        );
        let post = client.get_post(&post_id);
        assert_eq!(post.title, String::from_str(&env, "New title"));
    }
}